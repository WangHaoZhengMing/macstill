use macstill_hyprland as hyprland;

#[test]
fn ipc_rejects_an_address_with_extra_commands() {
    assert!(hyprland::focus("0x123; exit").is_err());
    assert!(hyprland::focus("garbage").is_err());
}
