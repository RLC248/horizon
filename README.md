# Launcher world picker proof for Horizon issue 20444

Windows launcher built with `cargo build --locked --manifest-path launcher/Cargo.toml --bin horizon-launcher` from d93e0942dcd6f333f47a42a60bcec61dff8a21f3.

A scratch Horizon instance held twelve synthetic worlds, including two named New World with the folders New World and New World (1).

1. Open Mods → Data packs and expand the installation world picker.
2. Confirm both New World entries show their distinct save folders.
3. Select the entry for New World (1). Confirm the collapsed picker now names that folder.
4. Reopen, scroll inside the list to Test World 12, and select it. Confirm the picker names world-12.
