use super::super::*;
use macstill_capture::{Service, Target};
use std::{collections::BTreeMap, sync::Arc};

pub(super) fn target(window: &crate::hyprland::stage::Window) -> Target {
    Target {
        address: window.address.clone(),
        pid: window.pid,
        class: window.class.clone(),
    }
}
pub(super) struct Content {
    rule_key: Option<String>,
    pub native: Option<super::native::Native>,
    service: Option<Service>,
    desired: Vec<Target>,
    pub images: BTreeMap<Target, (u64, Arc<RenderImage>)>,
    pub error: Option<String>,
}
impl Global for Content {}
impl Content {
    pub fn new() -> Self {
        let native = super::native::Native::connect();
        static NEXT_RULE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let key = format!(
            "_macstill_stage_capture_{}_{}",
            std::process::id(),
            NEXT_RULE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let name = format!("macstill-stage-{}.*", std::process::id());
        let rule = format!(
            "eval if {key} then {key}:set_enabled(true) else {key}=hl.window_rule({{name=\"{key}\",match={{workspace=\"name:{name}\"}},render_unfocused=true}}) end"
        );
        let error = crate::hyprland::request(&rule).err().map(|e| e.to_string());
        Self {
            rule_key: Some(key),
            service: if native.is_none() {
                Some(Service::new())
            } else {
                None
            },
            native,
            desired: Vec::new(),
            images: BTreeMap::new(),
            error,
        }
    }
    #[cfg(test)]
    pub fn fixture() -> Self {
        Self {
            rule_key: None,
            native: None,
            service: None,
            desired: Vec::new(),
            images: BTreeMap::new(),
            error: None,
        }
    }
    pub fn fallback(&mut self) {
        self.native = None;
        self.service = Some(Service::new());
        self.desired.clear();
    }
    pub fn refresh(&mut self, stage: &crate::hyprland::stage::Snapshot) -> bool {
        if self.native.is_some() {
            return true;
        }
        let desired: Vec<_> = stage
            .groups
            .iter()
            .flat_map(|g| g.windows.iter().take(3))
            .map(target)
            .take(24)
            .collect();
        if desired != self.desired {
            if let Some(service) = &self.service {
                service.update(desired.clone());
            }
            self.desired = desired;
            self.images.retain(|key, _| self.desired.contains(key));
        }
        let Some(service) = &self.service else {
            return false;
        };
        let snapshot = service.snapshot();
        let mut changed = snapshot.error != self.error;
        self.error = snapshot.error;
        for (target, texture) in snapshot.frames {
            if !self.desired.contains(&target)
                || self
                    .images
                    .get(&target)
                    .is_some_and(|(revision, _)| *revision == texture.revision)
            {
                continue;
            }
            // GPUI's atlas accepts premultiplied BGRA. wl_shm ARGB already has premultiplied alpha.
            let mut image = (*texture.rgba).clone();
            for pixel in image.pixels_mut() {
                pixel.0.swap(0, 2);
            }
            self.images.insert(
                target,
                (
                    texture.revision,
                    Arc::new(RenderImage::new(vec![image::Frame::new(image)])),
                ),
            );
            changed = true;
        }
        changed
    }
    pub fn image(&self, window: &crate::hyprland::stage::Window) -> Option<Arc<RenderImage>> {
        self.images
            .get(&target(window))
            .map(|(_, image)| image.clone())
    }
}
impl Drop for Content {
    fn drop(&mut self) {
        if self.service.is_none() && self.native.is_none() {
            return;
        }
        let Some(key) = &self.rule_key else {
            return;
        };
        let _ =
            crate::hyprland::request(&format!("eval if {key} then {key}:set_enabled(false) end"));
    }
}
