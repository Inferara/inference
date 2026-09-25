# Third-party notices

`infs` runs a `spacewasm` build in process, under the SpaceWasm interpreter,
which it links through `inference-spacewasm-runner`. The interpreter and the one
crate it depends on are compiled into the `infs` binary, and their licenses ask
that anyone who receives that binary also receives these texts:

| Crate | Version | License | Files |
|---|---|---|---|
| `spacewasm` | 0.7.1 | Apache-2.0 | `spacewasm/LICENSE`, `spacewasm/NOTICE` |
| `libm` | 0.2.16 | MIT | `libm/LICENSE.txt` |

Distributing `infs` distributes `spacewasm` in object form. Apache-2.0 §4(a)
asks for a copy of the license to go with it, and because `spacewasm` ships a
`NOTICE` file, §4(d) asks for a readable copy of that file's attribution notices
too, in a `NOTICE` file distributed with the binary. `libm` is MIT-licensed,
which asks for its copyright and permission notice in every copy; its
`LICENSE.txt` carries both, along with the copyright notices of the works it
derives from.

Every release archive carries this directory as `licenses/`, next to the
binaries: the `infs` archive, and the `infc` archive as well, although neither
`infc` nor `inference-lsp` links either crate.

This directory holds the texts for these two crates only. The release binaries
link other third-party crates, whose license texts are not collected here.

## Provenance

Each file is a byte-for-byte copy of the file of the same name in the crate's
package as published on crates.io, whose checksum `ci/rocq-discharge.cargo-lock`
records:

| File | SHA-256 | From the package | Package SHA-256 |
|---|---|---|---|
| `spacewasm/NOTICE` | `b5e8aff7775f17921056ee8af57c75ad69cb83a4c9b99262fe33ad260f8011b1` | `spacewasm-0.7.1.crate` | `53f8bfaf73590037468fc77f1a5d4baa3389e3aa73f72f18cc0f463c42c74d2e` |
| `spacewasm/LICENSE` | `c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4` | `spacewasm-0.7.1.crate` | `53f8bfaf73590037468fc77f1a5d4baa3389e3aa73f72f18cc0f463c42c74d2e` |
| `libm/LICENSE.txt` | `3823dda7cf046602f4b4e77ec8e227863dc4736037cc85bb33d9f19febe16bb7` | `libm-0.2.16.crate` | `b6d2cec3eae94f9f509c767b45932f1ada8350c4bdb85af2fcab4a3c14807981` |

`spacewasm`'s `NOTICE` ends without a newline, as upstream's does. The
repository's `.gitattributes` exempts this directory from line-ending
conversion, so every platform's checkout, and so every platform's archive,
carries these exact bytes.

When the `spacewasm` pin in the workspace `Cargo.toml` moves, copy the files
again from the new package, `libm`'s from the version that package pins, and
update both tables.
