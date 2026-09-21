# Synthetic fixture plan

S00 uses no downloaded or bundled artwork. Project code and documentation are
MIT-licensed; no art assets or generated fixture packages are committed to this
repository. The S03 fixture set will be generated locally from a checked-in
deterministic generator, after the proposed scene format and PNG decoding rules
are approved.

The first reference package will contain:

- a 3 × 2, 8-bit indexed image with raw indices `0`, `1`, `2`, and `255`,
- a 256 × 1 opaque RGBA8 palette whose entries 0, 1, 2, and 255 are distinct,
- no transparency, animation, weather, or location data,
- expected index bytes, palette bytes, and cover-scaling vectors in text form.

The generator will write only the package and its SHA-256 manifest. It will use
original synthetic values authored for this project; it will not download,
copy, or redistribute inspiration/demo artwork. Fixture provenance is synthetic and original. Generated outputs remain outside
the repository and are not distributed as project assets; only the generator,
text vectors, and provenance/hash instructions may be committed after the
scene schema and decoder contract are frozen in S03.
