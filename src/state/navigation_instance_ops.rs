impl NavState {
pub(crate) fn instance(&self, id: ScreenInstanceId) -> Option<&ScreenInstance> {
    if self.current.id == id {
        return Some(&self.current);
    }
    self.stack
        .iter()
        .map(SuspendedInstance::instance)
        .find(|instance| instance.id == id)
}

pub(crate) fn instance_mut(&mut self, id: ScreenInstanceId) -> Option<&mut ScreenInstance> {
    if self.current.id == id {
        return Some(&mut self.current);
    }
    self.stack
        .iter_mut()
        .map(SuspendedInstance::instance_mut)
        .find(|instance| instance.id == id)
}

pub(crate) fn instance_for_panel_mut(
    &mut self,
    panel: super::provider_panels::PanelInstanceId,
) -> Option<&mut ScreenInstance> {
    if self.current.provider_panels().lifecycle(panel).is_some() {
        return Some(&mut self.current);
    }
    self.stack
        .iter_mut()
        .map(SuspendedInstance::instance_mut)
        .find(|instance| instance.provider_panels().lifecycle(panel).is_some())
}

pub(crate) fn for_each_instance_mut(&mut self, mut visit: impl FnMut(&mut ScreenInstance)) {
    visit(&mut self.current);
    for suspended in &mut self.stack {
        visit(suspended.instance_mut());
    }
}
}
