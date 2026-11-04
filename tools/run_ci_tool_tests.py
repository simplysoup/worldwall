import os, sys, unittest
# Match `python3 -m unittest discover -s tools/tests` run from the repo root:
# the repo root must be importable so tests can `import tools.*`.
sys.path.insert(0, os.getcwd())
loader = unittest.TestLoader()
suite = loader.discover("tools/tests", pattern="test_*.py")
# test_worldwall_theme_watch is host-only: it needs the APCA Rust helper
# sources under ~/.local/share/worldwall-theme-sync/apca-helper, which are
# not in this repository, so it cannot run in a fresh checkout or CI.
def strip(s):
    for t in list(s):
        if isinstance(t, unittest.TestSuite):
            strip(t)
        elif t.__class__.__module__.startswith("test_worldwall_theme_watch"):
            s._tests.remove(t)
strip(suite)
result = unittest.TextTestRunner(verbosity=1).run(suite)
sys.exit(0 if result.wasSuccessful() else 1)
