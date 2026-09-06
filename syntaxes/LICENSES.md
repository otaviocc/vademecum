# Bundled syntax definitions

The `.sublime-syntax` files in this directory are third-party work, vendored so
that a vademecum binary is a complete install. They are compiled into the syntax
set at build time by `build.rs`; see README.md → Syntax highlighting.

Every file is redistributed under a licence that permits it, and every licence
here is compatible with vademecum's own MIT licence. Each entry names the
upstream commit the file was taken at, so it can be checked or updated.

| File | Upstream | Commit | Licence |
| --- | --- | --- | --- |
| `Swift.sublime-syntax` | [aerobounce/Swift-Next](https://github.com/aerobounce/Swift-Next) | `258b6249d8c8` | MIT |
| `Kotlin.sublime-syntax` | [guille/sublime-kotlin](https://github.com/guille/sublime-kotlin) | `c353694169c0` | Unlicense (public domain) |
| `TOML.sublime-syntax` | [sublimehq/Packages](https://github.com/sublimehq/Packages) | `f29821e2f98f` | Sublime HQ Packages licence (below) |
| `TypeScript.sublime-syntax` | [sharkdp/bat](https://github.com/sharkdp/bat) | `d7b651942287` | Apache-2.0 |

## Notes on individual files

**Swift.** Two others were tried first, and the reasons are worth keeping.

[colinta/decent-swift-syntax](https://github.com/colinta/decent-swift-syntax)
cannot be used at all: it matches hex and decimal float literals with regex
*subroutine calls* (`\g<1>`), which the pure-Rust `fancy-regex` engine does not
implement, and vademecum uses `fancy-regex` deliberately to avoid a C dependency
on Oniguruma. It fails to load outright with
`FeatureNotYetSupported("Subroutine Call")`.

[wbond/swift-for-sublime](https://github.com/wbond/swift-for-sublime) loads, and
looks like a safe choice, but covers literals only: its `expression` context
includes whitespace, strings and numbers and never the `identifier` context it
defines, and the file contains no `keyword` scopes at all. A fence of
`import`/`class`/`func`/`if`/`return` came out in a single colour — the symptom
this whole directory exists to fix, still present for one language.

So the rule for a replacement is both: it has to load under `fancy-regex`, *and*
it has to colour a keyword-only sample. The test in `src/render/code.rs` uses
samples with no string or number literal in them for exactly that reason — a
sample containing `"hi"` passes on the strength of the one string.

**TypeScript.** Taken from `bat`, which converted it by hand from
[Microsoft/TypeScript-Sublime-Plugin](https://github.com/Microsoft/TypeScript-Sublime-Plugin)
(also Apache-2.0) — that upstream ships only `.tmLanguage`, and syntect 5.3 can
load plists for *themes* but not for syntaxes. The definition Sublime Text ships
today cannot be used either: it is `extends:`-based, chaining through
`TypeScript (Plain)` to a modern `JavaScript (Plain)`, so vendoring it would mean
replacing syntect's bundled JavaScript wholesale. This file is self-contained.

## Sublime HQ Packages licence

Applies to `TOML.sublime-syntax`, quoted from
<https://github.com/sublimehq/Packages/blob/master/LICENSE>:

> If not otherwise specified (see below), files in this repository fall under
> the following license:
>
>     Permission to copy, use, modify, sell and distribute this
>     software is granted. This software is provided "as is" without
>     express or implied warranty, and with no claim as to its
>     suitability for any purpose.
>
> An exception is made for files in readable text which contain their own
> license information, or files where an accompanying file exists (in the same
> directory) with a "-license" suffix added to the base-name name of the
> original file, and an extension of txt, html, or similar.

`TOML.sublime-syntax` carries no licence header of its own and has no
accompanying `-license` file upstream, so the terms above are the ones that
apply to it.
