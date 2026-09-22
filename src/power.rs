//! Suspend/resume state and the systemd-logind power-event adapter.
//!
//! The state machine is deterministic and has no Wayland or renderer
//! dependencies.  The adapter is a best-effort system-bus subscriber: it
//! reports missing/disconnected logind as a degraded state instead of
//! fabricating a resume.  D-Bus work is kept on a worker thread so the
//! Wayland dispatch thread never waits on the system bus.

use std::{sync::mpsc::TrySendError, thread};

use calloop::channel::{self, Channel, SyncSender};
use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator},
    message::Type,
};

const LOGIND_NAME: &str = "org.freedesktop.login1";
const LOGIND_INTERFACE: &str = "org.freedesktop.login1.Manager";
const DBUS_INTERFACE: &str = "org.freedesktop.DBus";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerEvent {
    EnteringSleep,
    Resumed,
    ServiceAvailable,
    ServiceUnavailable,
    Disconnected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerServiceState {
    Unknown,
    Available,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPhase {
    Active,
    Suspended,
    Resuming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerAction {
    Pause { generation: u64 },
    BeginResume { generation: u64 },
    ServiceDegraded,
    ServiceRestored,
    Ignored,
}

/// Coalesces logind's boolean PrepareForSleep signals and hands the caller a
/// generation token for one resume refresh.  Output readiness is deliberately
/// supplied separately: a resume signal may arrive while a layer configure is
/// incomplete or an output has already gone away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerStateMachine {
    service: PowerServiceState,
    phase: PowerPhase,
    generation: u64,
    resume_started: bool,
}

impl Default for PowerStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl PowerStateMachine {
    pub const fn new() -> Self {
        Self {
            service: PowerServiceState::Unknown,
            phase: PowerPhase::Active,
            generation: 0,
            resume_started: false,
        }
    }

    pub const fn service(self) -> PowerServiceState {
        self.service
    }

    pub const fn phase(self) -> PowerPhase {
        self.phase
    }

    pub const fn generation(self) -> u64 {
        self.generation
    }

    pub fn on_event(&mut self, event: PowerEvent) -> PowerAction {
        match event {
            PowerEvent::EnteringSleep => {
                if self.phase == PowerPhase::Suspended {
                    return PowerAction::Ignored;
                }
                self.generation = self.generation.saturating_add(1);
                self.phase = PowerPhase::Suspended;
                self.resume_started = false;
                PowerAction::Pause {
                    generation: self.generation,
                }
            }
            PowerEvent::Resumed => {
                if self.phase != PowerPhase::Suspended {
                    return PowerAction::Ignored;
                }
                self.generation = self.generation.saturating_add(1);
                self.phase = PowerPhase::Resuming;
                self.resume_started = false;
                PowerAction::BeginResume {
                    generation: self.generation,
                }
            }
            PowerEvent::ServiceAvailable => {
                let was_available = self.service == PowerServiceState::Available;
                self.service = PowerServiceState::Available;
                if was_available {
                    PowerAction::Ignored
                } else {
                    PowerAction::ServiceRestored
                }
            }
            PowerEvent::ServiceUnavailable | PowerEvent::Disconnected => {
                let was_degraded = self.service == PowerServiceState::Degraded;
                self.service = PowerServiceState::Degraded;
                if was_degraded {
                    PowerAction::Ignored
                } else {
                    PowerAction::ServiceDegraded
                }
            }
        }
    }

    /// Start the one resume refresh once the selected output has a valid
    /// physical extent.  Calling this repeatedly while a resume is pending is
    /// intentionally idempotent.
    pub fn begin_resume_if_ready(&mut self, output_ready: bool) -> Option<u64> {
        if output_ready && self.phase == PowerPhase::Resuming && !self.resume_started {
            self.resume_started = true;
            Some(self.generation)
        } else {
            None
        }
    }

    /// Complete a resume only for the current generation.  A sleep arriving
    /// while a refresh is in progress invalidates the old token.
    pub fn complete_resume(&mut self, generation: u64) -> bool {
        if self.phase == PowerPhase::Resuming
            && self.resume_started
            && self.generation == generation
        {
            self.phase = PowerPhase::Active;
            self.resume_started = false;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
pub struct PowerEventSource {
    channel: Option<Channel<PowerEvent>>,
}

impl PowerEventSource {
    /// Start the real system-bus adapter.  Startup failures are delivered as
    /// `Disconnected`/`ServiceUnavailable` events rather than returned as a
    /// fatal application error, preserving manual/rendering operation.
    pub fn start() -> Self {
        let (sender, channel) = channel::sync_channel(32);
        spawn_prepare_listener(sender.clone());
        spawn_owner_listener(sender);
        Self {
            channel: Some(channel),
        }
    }

    pub fn into_event_source(mut self) -> Channel<PowerEvent> {
        self.channel
            .take()
            .expect("power event source already consumed")
    }
}

fn spawn_prepare_listener(sender: SyncSender<PowerEvent>) {
    let thread_sender = sender.clone();
    let result = thread::Builder::new()
        .name("worldwall-power-events".to_owned())
        .spawn(move || {
            let sender = thread_sender;
            let connection = match Connection::system() {
                Ok(connection) => connection,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=system-bus error={error}"
                    );
                    return;
                }
            };
            let rule = match MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(LOGIND_NAME)
                .and_then(|builder| builder.interface(LOGIND_INTERFACE))
                .and_then(|builder| builder.member("PrepareForSleep"))
                .map(|builder| builder.build())
            {
                Ok(rule) => rule,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=match-rule error={error}"
                    );
                    return;
                }
            };
            let messages = match MessageIterator::for_match_rule(rule, &connection, Some(8)) {
                Ok(messages) => messages,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=subscription error={error}"
                    );
                    return;
                }
            };
            for message in messages {
                match message {
                    Ok(message) => match message.body().deserialize::<bool>() {
                        Ok(true) => send_event(&sender, PowerEvent::EnteringSleep),
                        Ok(false) => send_event(&sender, PowerEvent::Resumed),
                        Err(error) => eprintln!(
                            "trace power-adapter ignored=malformed-prepare-for-sleep error={error}"
                        ),
                    },
                    Err(error) => {
                        send_event(&sender, PowerEvent::Disconnected);
                        eprintln!(
                            "trace power-adapter state=disconnected reason=receive error={error}"
                        );
                        return;
                    }
                }
            }
            send_event(&sender, PowerEvent::Disconnected);
            eprintln!("trace power-adapter state=disconnected reason=subscription-ended");
        });
    if let Err(error) = result {
        send_event(&sender, PowerEvent::Disconnected);
        eprintln!("trace power-adapter state=degraded reason=thread-start error={error}");
    }
}

fn spawn_owner_listener(sender: SyncSender<PowerEvent>) {
    let thread_sender = sender.clone();
    let result = thread::Builder::new()
        .name("worldwall-power-service".to_owned())
        .spawn(move || {
            let sender = thread_sender;
            let connection = match Connection::system() {
                Ok(connection) => connection,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=system-bus error={error}"
                    );
                    return;
                }
            };
            match connection.call_method(
                Some(DBUS_INTERFACE),
                "/org/freedesktop/DBus",
                Some(DBUS_INTERFACE),
                "NameHasOwner",
                &LOGIND_NAME,
            ) {
                Ok(reply) => match reply.body().deserialize::<bool>() {
                    Ok(true) => send_event(&sender, PowerEvent::ServiceAvailable),
                    Ok(false) => {
                        send_event(&sender, PowerEvent::ServiceUnavailable);
                        eprintln!("trace power-service state=missing adapter=systemd-logind");
                    }
                    Err(error) => {
                        eprintln!("trace power-adapter ignored=malformed-name-owner error={error}")
                    }
                },
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=name-owner error={error}"
                    );
                    return;
                }
            }
            let rule = match MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(DBUS_INTERFACE)
                .and_then(|builder| builder.interface(DBUS_INTERFACE))
                .and_then(|builder| builder.member("NameOwnerChanged"))
                .and_then(|builder| builder.add_arg(LOGIND_NAME))
                .map(|builder| builder.build())
            {
                Ok(rule) => rule,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=match-rule error={error}"
                    );
                    return;
                }
            };
            let messages = match MessageIterator::for_match_rule(rule, &connection, Some(8)) {
                Ok(messages) => messages,
                Err(error) => {
                    send_event(&sender, PowerEvent::Disconnected);
                    eprintln!(
                        "trace power-adapter state=disconnected reason=subscription error={error}"
                    );
                    return;
                }
            };
            for message in messages {
                match message {
                    Ok(message) => match message.body().deserialize::<(String, String, String)>() {
                        Ok((_, _, new_owner)) if new_owner.is_empty() => {
                            send_event(&sender, PowerEvent::ServiceUnavailable);
                            eprintln!("trace power-service state=missing adapter=systemd-logind");
                        }
                        Ok((_, _, _)) => send_event(&sender, PowerEvent::ServiceAvailable),
                        Err(error) => eprintln!(
                            "trace power-adapter ignored=malformed-name-owner error={error}"
                        ),
                    },
                    Err(error) => {
                        send_event(&sender, PowerEvent::Disconnected);
                        eprintln!(
                            "trace power-adapter state=disconnected reason=receive error={error}"
                        );
                        return;
                    }
                }
            }
            send_event(&sender, PowerEvent::Disconnected);
            eprintln!("trace power-adapter state=disconnected reason=subscription-ended");
        });
    if let Err(error) = result {
        send_event(&sender, PowerEvent::Disconnected);
        eprintln!("trace power-adapter state=degraded reason=thread-start error={error}");
    }
}

fn send_event(sender: &SyncSender<PowerEvent>, event: PowerEvent) {
    match sender.try_send(event) {
        Ok(()) | Err(TrySendError::Full(_)) => {}
        Err(TrySendError::Disconnected(_)) => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PausedAnimationClock {
    origin_ns: u64,
    paused_at_ns: Option<u64>,
    suspended_ns: u64,
}

impl PausedAnimationClock {
    pub const fn new(origin_ns: u64) -> Self {
        Self {
            origin_ns,
            paused_at_ns: None,
            suspended_ns: 0,
        }
    }

    pub fn pause(&mut self, now_ns: u64) {
        if self.paused_at_ns.is_none() {
            self.paused_at_ns = Some(now_ns);
        }
    }

    pub fn resume(&mut self, now_ns: u64) {
        if let Some(paused_at) = self.paused_at_ns.take() {
            self.suspended_ns = self
                .suspended_ns
                .saturating_add(now_ns.saturating_sub(paused_at));
        }
    }

    pub fn seconds(&self, now_ns: u64) -> f32 {
        let end = self.paused_at_ns.unwrap_or(now_ns);
        ((end
            .saturating_sub(self.origin_ns)
            .saturating_sub(self.suspended_ns) as f64
            / 1_000_000_000.0)
            % 1024.0) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_sleep_and_resume_signals_are_coalesced() {
        let mut machine = PowerStateMachine::new();
        assert_eq!(
            machine.on_event(PowerEvent::EnteringSleep),
            PowerAction::Pause { generation: 1 }
        );
        assert_eq!(
            machine.on_event(PowerEvent::EnteringSleep),
            PowerAction::Ignored
        );
        assert_eq!(
            machine.on_event(PowerEvent::Resumed),
            PowerAction::BeginResume { generation: 2 }
        );
        assert_eq!(machine.on_event(PowerEvent::Resumed), PowerAction::Ignored);
        assert_eq!(machine.begin_resume_if_ready(false), None);
        assert_eq!(machine.begin_resume_if_ready(true), Some(2));
        assert_eq!(machine.begin_resume_if_ready(true), None);
        assert!(machine.complete_resume(2));
        assert_eq!(machine.phase(), PowerPhase::Active);
    }

    #[test]
    fn sleep_invalidates_a_pending_resume_generation() {
        let mut machine = PowerStateMachine::new();
        machine.on_event(PowerEvent::EnteringSleep);
        machine.on_event(PowerEvent::Resumed);
        assert_eq!(machine.begin_resume_if_ready(true), Some(2));
        assert_eq!(
            machine.on_event(PowerEvent::EnteringSleep),
            PowerAction::Pause { generation: 3 }
        );
        assert!(!machine.complete_resume(2));
        assert_eq!(machine.phase(), PowerPhase::Suspended);
    }

    #[test]
    fn missing_service_is_degraded_without_a_false_resume() {
        let mut machine = PowerStateMachine::new();
        assert_eq!(
            machine.on_event(PowerEvent::ServiceUnavailable),
            PowerAction::ServiceDegraded
        );
        assert_eq!(machine.service(), PowerServiceState::Degraded);
        assert_eq!(machine.phase(), PowerPhase::Active);
        assert_eq!(machine.on_event(PowerEvent::Resumed), PowerAction::Ignored);
        assert_eq!(
            machine.on_event(PowerEvent::ServiceAvailable),
            PowerAction::ServiceRestored
        );
    }

    #[test]
    fn paused_animation_clock_excludes_sleep_duration_and_is_idempotent() {
        let mut clock = PausedAnimationClock::new(100);
        assert!((clock.seconds(1_000_000_100) - 1.0).abs() < f32::EPSILON);
        clock.pause(2_000_000_100);
        clock.pause(3_000_000_100);
        assert!((clock.seconds(5_000_000_100) - 2.0).abs() < f32::EPSILON);
        clock.resume(5_000_000_100);
        clock.resume(6_000_000_100);
        assert!((clock.seconds(6_000_000_100) - 3.0).abs() < f32::EPSILON);
    }
}
