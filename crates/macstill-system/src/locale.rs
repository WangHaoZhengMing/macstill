//! System defaults through localed. These do not reconfigure a running Wayland compositor.
use crate::{dbus::*, error::*, model::*};
use gio::glib::variant::ToVariant;
const SERVICE: &str = "org.freedesktop.locale1";
const PATH: &str = "/org/freedesktop/locale1";
pub struct LocaleSettings<'a, T: Transport>(pub &'a T);
impl<T: Transport> LocaleSettings<'_, T> {
    pub fn snapshot(&self) -> Result<LocaleState> {
        let p = get_all(self.0, Bus::System, SERVICE, PATH, SERVICE)?;
        Ok(LocaleState {
            locale: field(&p, "Locale")?,
            x11: X11Keyboard {
                layout: field(&p, "X11Layout")?,
                model: field(&p, "X11Model")?,
                variant: field(&p, "X11Variant")?,
                options: field(&p, "X11Options")?,
            },
            console: ConsoleKeyboard {
                keymap: field(&p, "VConsoleKeymap")?,
                keymap_toggle: field(&p, "VConsoleKeymapToggle")?,
            },
        })
    }
    /// Replaces all system locale assignments; an empty list clears overrides.
    pub fn set_locale(&self, assignments: &[String]) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for assignment in assignments {
            let Some((key, value)) = assignment.split_once('=') else {
                return Err(SystemError::invalid(
                    "locale",
                    "Use NAME=locale assignments",
                ));
            };
            if !matches!(
                key,
                "LANG"
                    | "LC_CTYPE"
                    | "LC_NUMERIC"
                    | "LC_TIME"
                    | "LC_COLLATE"
                    | "LC_MONETARY"
                    | "LC_MESSAGES"
                    | "LC_PAPER"
                    | "LC_NAME"
                    | "LC_ADDRESS"
                    | "LC_TELEPHONE"
                    | "LC_MEASUREMENT"
                    | "LC_IDENTIFICATION"
            ) || !seen.insert(key)
                || !token(value, b"_.@+-", 128)
            {
                return Err(SystemError::invalid(
                    "locale",
                    "Use unique LANG or LC_* assignments with valid locale names; LC_ALL is not persistent",
                ));
            }
        }
        self.write("SetLocale", (assignments, true).to_variant())
    }
    pub fn set_x11_keyboard(&self, keyboard: &X11Keyboard, convert: bool) -> Result<()> {
        if keyboard.layout.is_empty()
            || !token(&keyboard.layout, b"_-,", 256)
            || keyboard.layout.split(',').any(str::is_empty)
            || !token(&keyboard.model, b"_-", 128)
            || !token(&keyboard.variant, b"_-,", 256)
            || (!keyboard.variant.is_empty()
                && keyboard.variant.split(',').count() != keyboard.layout.split(',').count())
            || !token(&keyboard.options, b"_-,:", 1024)
        {
            return Err(SystemError::invalid(
                "keyboard",
                "Choose XKB layouts, matching variants, a model and XKB options",
            ));
        }
        self.write(
            "SetX11Keyboard",
            (
                &keyboard.layout,
                &keyboard.model,
                &keyboard.variant,
                &keyboard.options,
                convert,
                true,
            )
                .to_variant(),
        )
    }
    pub fn set_console_keyboard(&self, keyboard: &ConsoleKeyboard, convert: bool) -> Result<()> {
        if !token(&keyboard.keymap, b"_-.+", 128) || !token(&keyboard.keymap_toggle, b"_-.+", 128) {
            return Err(SystemError::invalid(
                "console keyboard",
                "Use console keymap names, not file paths",
            ));
        }
        self.write(
            "SetVConsoleKeyboard",
            (&keyboard.keymap, &keyboard.keymap_toggle, convert, true).to_variant(),
        )
    }
    fn write(&self, method: &str, args: gio::glib::Variant) -> Result<()> {
        unit(
            self.0
                .call(Bus::System, SERVICE, PATH, SERVICE, method, args, true)?,
            method,
        )
    }
}
fn token(value: &str, punctuation: &[u8], max: usize) -> bool {
    value.len() <= max
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || punctuation.contains(&c))
}
