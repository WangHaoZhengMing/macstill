use gpui_kit::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub dark: bool,
    pub background: u32,
    pub surface: u32,
    pub sidebar: u32,
    pub text: u32,
    pub muted: u32,
    pub line: u32,
    pub hover: u32,
    pub accent: u32,
    pub accent_text: u32,
    pub switch_off: u32,
}
/// Regional tint over the compositor-filtered backdrop; never apply opacity to
/// the whole widget, which would also wash out text and icons.
#[derive(Clone, Copy)]
pub enum Material {
    Chrome,
    Menu,
    Panel,
}
impl Palette {
    pub fn material(self, material: Material) -> Hsla {
        let color = match (self.dark, material) {
            (false, Material::Chrome) => 0xf2f2f278,
            (true, Material::Chrome) => 0x20202488,
            (false, Material::Menu) => 0xf4f4f4b0,
            (true, Material::Menu) => 0x252529b8,
            (false, Material::Panel) => 0xf5f5faa0,
            (true, Material::Panel) => 0x25252da8,
        };
        rgba(color).into()
    }
    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                dark,
                background: 0x252527,
                surface: 0x323234,
                sidebar: 0x202023,
                text: 0xf4f4f6,
                muted: 0xaaaaaf,
                line: 0x454548,
                hover: 0x414145,
                accent: 0x3478f6,
                accent_text: 0xffffff,
                switch_off: 0x66666b,
            }
        } else {
            Self {
                dark,
                background: 0xf5f5f7,
                surface: 0xffffff,
                sidebar: 0xe9e9ed,
                text: 0x242427,
                muted: 0x7c7c83,
                line: 0xdddde2,
                hover: 0xededf2,
                accent: 0x3478f6,
                accent_text: 0xffffff,
                switch_off: 0xbcbcc2,
            }
        }
    }
}

/// Theme installed by this library, also projected into gpui-base's semantic tokens.
#[derive(Clone, Copy)]
pub struct Theme(pub Palette);
impl Global for Theme {}

impl Palette {
    pub fn current(cx: &App) -> Self {
        cx.try_global::<Theme>()
            .map(|theme| theme.0)
            .unwrap_or_else(|| Self::new(false))
    }

    pub fn tokens(self) -> gpui_kit::base::SemanticThemeTokens {
        let mut tokens = gpui_kit::base::SemanticThemeTokens {
            colors: if self.dark {
                gpui_kit::base::ColorTokens::dark()
            } else {
                gpui_kit::base::ColorTokens::light()
            },
            ..Default::default()
        };
        tokens.colors.background = rgb(self.background).into();
        tokens.colors.foreground = rgb(self.text).into();
        tokens.colors.surface = rgb(self.surface).into();
        tokens.colors.surface_foreground = rgb(self.text).into();
        tokens.colors.primary = rgb(self.accent).into();
        tokens.colors.primary_foreground = rgb(self.accent_text).into();
        tokens.colors.secondary = rgb(self.sidebar).into();
        tokens.colors.secondary_foreground = rgb(self.text).into();
        tokens.colors.muted = rgb(self.hover).into();
        tokens.colors.muted_foreground = rgb(self.muted).into();
        tokens.colors.accent = rgb(self.accent).into();
        tokens.colors.accent_foreground = rgb(self.accent_text).into();
        tokens.colors.border = rgb(self.line).into();
        tokens.colors.input = rgb(self.line).into();
        tokens.colors.ring = rgb(self.accent).into();
        tokens.colors.selection = Hsla::from(rgb(self.accent)).alpha(0.3);
        tokens.radius.md = px(7.);
        tokens.radius.lg = px(8.);
        tokens.radius.xl = px(12.);
        tokens.typography.sm.size = px(12.);
        tokens.typography.sm.line_height = px(17.);
        tokens.typography.md.size = px(13.);
        tokens.typography.md.line_height = px(18.);
        tokens
    }
}

/// Update the visual theme and the gpui-base editor/primitive defaults together.
pub fn set_theme(palette: Palette, cx: &mut App) {
    if cx
        .try_global::<Theme>()
        .is_some_and(|theme| theme.0 == palette)
    {
        return;
    }
    let mut tokens = palette.tokens();
    if let Some(fonts) = cx.try_global::<crate::Fonts>() {
        tokens.typography.sans = fonts.text.clone();
        tokens.typography.mono = fonts.mono.clone();
    }
    let base = gpui_kit::base::Theme::global_mut(cx);
    base.tokens = tokens;
    base.appearance = if palette.dark {
        gpui_kit::base::ThemeAppearance::Dark
    } else {
        gpui_kit::base::ThemeAppearance::Light
    };
    cx.set_global(Theme(palette));
    cx.refresh_windows();
}
