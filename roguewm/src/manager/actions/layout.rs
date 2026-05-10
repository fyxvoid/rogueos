use anyhow::Result;
use crate::roguewm::manager::WindowManager;
use crate::roguewm::config::LayoutSymbol;

impl<'a> WindowManager<'a> {
    pub fn set_layout(&mut self, layout: LayoutSymbol) -> Result<()> {
        let mon_idx = self.sel_mon;
        self.monitors[mon_idx].layout = layout;
        self.arrange()?;
        Ok(())
    }

    pub fn set_mfact(&mut self, f: f32) -> Result<()> {
        let mon_idx = self.sel_mon;
        let mon = &mut self.monitors[mon_idx];
        mon.mfact = (mon.mfact + f).min(0.95).max(0.05);
        self.arrange()?;
        Ok(())
    }

    pub fn toggle_gaps(&mut self) -> Result<()> {
        let mon_idx = self.sel_mon;
        self.monitors[mon_idx].show_gaps = !self.monitors[mon_idx].show_gaps;
        self.arrange()?;
        Ok(())
    }
}
