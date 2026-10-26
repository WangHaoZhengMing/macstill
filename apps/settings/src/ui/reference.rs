//! Curated 24A335 artwork; embedded so installed Settings has no research-folder dependency.
use super::*;
macro_rules! artwork {
    ($name:literal) => {
        include_bytes!(concat!(
            "../../../../assets/icons/settings/reference/",
            $name
        )) as &[u8]
    };
}
fn image(bytes: &'static [u8]) -> Img {
    img(std::sync::Arc::new(Image::from_bytes(
        ImageFormat::Png,
        bytes.to_vec(),
    )))
}
pub(super) fn appearance(dark: bool) -> Img {
    image(if dark {
        artwork!("appearance-dark.png")
    } else {
        artwork!("appearance-light.png")
    })
}
pub(super) fn computer() -> Img {
    image(artwork!("computer.png"))
}
pub(super) fn bluetooth() -> Img {
    image(artwork!("bluetooth.png"))
}
pub(super) fn resolution(index: usize, dark: bool) -> Img {
    // Apple numbers these from smallest text to largest; the UI runs the other way.
    let bytes = match (4 - index.min(4), dark) {
        (0, false) => artwork!("resolution-1-light.png"),
        (1, false) => artwork!("resolution-2-light.png"),
        (2, false) => artwork!("resolution-3-light.png"),
        (3, false) => artwork!("resolution-4-light.png"),
        (4, false) => artwork!("resolution-5-light.png"),
        (0, true) => artwork!("resolution-1-dark.png"),
        (1, true) => artwork!("resolution-2-dark.png"),
        (2, true) => artwork!("resolution-3-dark.png"),
        (3, true) => artwork!("resolution-4-dark.png"),
        _ => artwork!("resolution-5-dark.png"),
    };
    image(bytes)
}
pub(super) fn general(label: &str) -> Option<Img> {
    Some(image(match label {
        "Software Update" => artwork!("software-update.png"),
        "Time Machine" => artwork!("time-machine.png"),
        "Startup Disk" => artwork!("startup-disk.png"),
        _ => return None,
    }))
}
