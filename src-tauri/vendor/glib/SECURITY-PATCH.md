This is GLib 0.18.5 from crates.io, retained for GTK 3 ABI compatibility.

The only source change backports gtk-rs/gtk-rs-core PR 1343: VariantStrIter passes its output pointer as `&mut p`, with a mutable binding. This fixes RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g.

Upstream fix: https://github.com/gtk-rs/gtk-rs-core/pull/1343

The optimized Linux release test exercises forward and reverse string iteration.
