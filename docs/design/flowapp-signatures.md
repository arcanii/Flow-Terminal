# Who signed each download

Asked for on 2026-11-20: flowapp's `list` and `search` should show each
download's signing key, who signed it and when, and there should be a
check anyone can run to see that a signature is valid. Both went on the
backlog and were built the same day.

Every flowapp download carries an Ed25519 signature by Flow's update key
over the whole file, which `apps::check` verifies before anything is
unpacked. flowapp showed none of it. The signature names no one and
carries no date, and `apps.json` itself isn't signed, only each download.

## What was decided

- **What to build:** the columns, and all three of `flowapp verify`, a
  check with OpenSSL that Flow-releases publishes, and code signatures
  in `info`.
- **The date:** the catalog's word, written by `app_catalog` as it adds
  a download, not signed. Someone able to edit `apps.json` could change a
  date, though not a download.
- **The layout:** one Signed column, `Flow f186 6b49 · 2026-11-18`. The
  full fingerprint goes in `info` and `--json`.
- **The signer's name:** "Flow".

## Keys and fingerprints (`apps/signing.rs`)

- **`KEYS`**: Flow's own table of the keys it takes, one today
  (`UPDATE_PUBLIC_KEY`, named "Flow"). `signer` gives a key's name, or
  none for a key not in the table. The name comes from this table, never
  from the catalog, since nothing signs the catalog.
- **`fingerprint`**: the SHA-256 of the key's DER SubjectPublicKeyInfo
  (the 12-byte prefix `302a300506032b6570032100` and the 32 raw bytes),
  in hex. So anyone can work it out from the PEM with OpenSSL:
  `openssl pkey -pubin -in flow-update.pem -outform DER | openssl dgst
  -sha256`. Flow's is
  `f1866b49b0b42871463772bf1c9d77fae2091c32dc01681ae5d752f13008b4d0`.
  The tables show its first eight digits as `f186 6b49`.
- **`pem`**: the key as a PEM file, which OpenSSL reads.
- **`code_signature`**: reads what `codesign -dvv` reports into a
  `CodeSigned`: signed (its first authority, team and timestamp), ad hoc,
  or not signed. Core only reads the report: `flowapp` runs `codesign`.

## The catalog's date, and a .sig beside each download

- **`Download::signed`** is a UTC date, `2026-11-18`, left out when
  empty. An older flowapp reads past it, as it does `flow`.
- **`examples/app_catalog.rs`** writes today's UTC date, or `--signed
  <date>`. Beside the tarball it writes `<file>.sig`, the signature's 64
  bytes, for the release.
- **The release scripts** print `gh release` commands that name the
  `.sig` too: Flow's three here, and those of each repository whose apps
  flowapp installs.

## What an install keeps

`installed.json`'s `Kept` gains four values, each left out when empty, so
an older file still reads:
- `download`: the catalog's entry it was installed from.
- `key`: the fingerprint of the key that checked it.
- `checked`: the UTC date it was checked.
- `own_file`: installed from a file of the user's own, so signed by no
  one.

`install_with` fills them, which covers `install`, `upgrade`,
`keep_carried` and Settings' Apps; `install_file` marks its own. An
older flowapp that rewrites `installed.json` drops them, and the
fallbacks below cover that.

## list, search, info and --json

- **The column:**
  - `list`: App, Installed, Latest, **Signed**. Its From column went:
    the Signed column says "with Flow" for a game an older Flow carried,
    which was all From told apart, and `--json` keeps `from`.
  - `search`: App, Installed, Latest, **Signed**, About. About is cut to
    fit first, then Signed once About is down to its least, so at 80
    columns About keeps only a few letters once the catalog has dates.
  - `info`'s Code signed row is two: who signed, then the files and
    when, so it fits 80 columns.
- **What it shows:**
  - `search` goes by the catalog's download for this system: the key
    flowapp will check it with, and the catalog's date.
  - `list` goes by what's installed: what `Kept` recorded, plain.
  - An app installed before `Kept` held them takes the catalog's
    download of the same version, dim; with none, nothing.
  - A package from a file says "not signed". A game an older Flow
    carried says "with Flow", dim.
  - A download with no date shows the name and key alone.
- **`info`'s rows:** Signed by (the name and the whole fingerprint),
  Signed on, Checked (for one installed), and on the Mac, Code signed.
  Under the table, the lines to check it yourself.
- **`--json`** gains `signed_by`, `key`, `signed`, `checked` and
  `signature`, and `info --json` also gains `code_signatures`. Keys stay
  English.

## flowapp verify

`flowapp verify [names…]` checks each installed app, or every one with no
names:
1. **Which download:** the one `Kept` recorded, else the catalog's of the
   same version (saying so), else it can't, and says to install it
   again.
2. **Fetch and check:** fetched as an install fetches, then checked with
   Flow's key.
3. **Unpack aside:** in a scratch folder in the system's temp directory,
   never in the apps' folder, removed after.
4. **Compare:** every file of `<name>/<version>/` by its SHA-256 (a
   link by its target), and each program's copy in `apps/bin/`.
   - A file changed or missing fails.
   - A file added since the install (Python's `__pycache__`, say) is
     listed but doesn't fail.
5. **Say it:** a line for each app, the differences under it.

A package from a file is signed by no one, and a game an older Flow
carried came with Flow, so neither is checked. The exit code is 0 when
all is well, 1 when something changed or failed, and 2 for a usage
mistake.

## Code signatures (the Mac)

`info` finds the Mach-O files in an installed app's version folder by
their first bytes, has `codesign -dvv` read each, and groups them by who
signed them: a Developer ID with its team, the files and a timestamp;
ad hoc; or not code-signed. Windows and Linux show no such row: Flow's
Windows downloads aren't code-signed, and Authenticode is for later.

## Checking it yourself

`info` prints the lines to check this system's download with OpenSSL 3
or later (the Mac's own `/usr/bin/openssl`, LibreSSL 3.3.6, can't read
an Ed25519 key):
1. fetch `flow-update.pem` from Flow-releases;
2. work out its fingerprint, which must be the one above;
3. fetch the download;
4. write its `.sig` from the catalog's signature, with OpenSSL's own
   base64;
5. `openssl pkeyutl -verify -pubin -inkey flow-update.pem -rawin -in
   <file> -sigfile <file>.sig`, which says "Signature Verified
   Successfully".

Tried on the Mac (2026-11-20) with flow-say 0.1.1 and Homebrew's OpenSSL
4.1.0: verified, and a copy with bytes added refused.

**Flow-releases**, published 2026-11-20 (`843dc79`):
- `flow-update.pem` at its root;
- a README section, "Checking a download yourself", with the
  fingerprint and the lines;
- dates filled into `apps.json` for the downloads already there, from
  each file's upload date on GitHub;
- a `.sig` beside each download already there, from `apps.json`'s
  signatures.

## In Settings' Apps (0.1.46)

Begun for 0.1.46 on 2026-11-23: a hint, and a mark where something's off
(decided 2026-11-23).

- **The hint** of the app chosen ends saying who signed it, as `info`
  does:
  - installed, as the install noted it: "Signed by Flow (key f186 6b49)
    on 2026-11-18, and checked with Flow's key as it was installed,
    2026-11-20.";
  - installed by a flowapp that noted nothing, the catalog's word for
    that version: "The catalog says Flow (key f186 6b49) signed this
    version on 2026-11-18. Nothing noted it as it was installed.";
  - not installed: "Its download is signed by Flow (key f186 6b49) on
    2026-11-18.";
  - a package of the user's own: "Not signed: installed from a file of
    your own.";
  - a game that came with Flow: "It came with Flow.";
  - installed, and neither the install nor the catalog says: "Nothing
    says who signed it."

  Where the catalog gives no date, the words leave it out. A key Flow
  doesn't know is "an unknown key", with its short fingerprint.
- **A mark on the row** only where something's off: "· not signed" for
  a file of the user's own, "· unknown key" for a key not in `KEYS`.
  A row signed by Flow stays as it was.
- **How:** `shelf::lines` takes what the install noted
  (`Shown::noting`, as `list` does), and `shelf::line` words it
  (`signed_hint`, `off_mark`). The made-up shelf for snapshots gains
  signatures and a package of the user's own, for `settings-apps.png`.

## Left for later

- Authenticode on Windows, once its downloads are code-signed.
- Live tries on Windows and Linux.
