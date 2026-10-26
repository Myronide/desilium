# Attribution

`crates/compositor/src/{state,input,winit,handlers,grabs}` derives from
[Smithay Smallvil](https://github.com/Smithay/smithay/tree/76118ad7796e3edc46875f44c437485996a8e39d/smallvil),
MIT, copyright 2017 Victor Berger and Victoria Brekenfeld. The original license
is retained in `LICENSE-SMITHAY`. WC adds settings, window policy, control IPC,
focus handling, shortcuts, placement, and application lifecycle handling.

The native settings application uses Dioxus Native 0.7 and Blitz 0.2
(MIT OR Apache-2.0). Dependencies and versions are recorded in Cargo.lock.
