use anyhow::Result;
use std::process::Command;
use crate::roguewm::manager::WindowManager;


impl<'a> WindowManager<'a> {
    pub fn quit(&self) {
        std::process::exit(0);
    }

    pub fn spawn(&self, cmd: &[&str]) -> Result<()> {
        if let Some((bin, args)) = cmd.split_first() {
            Command::new(bin).args(args).spawn().ok();
        }
        Ok(())
    }
}
