use anyhow::Result;

use x11rb::protocol::xproto::*;
use super::WindowManager;
use crate::roguewm::config::*;
use crate::roguewm::layouts::Rect;

impl<'a> WindowManager<'a> {
    pub(crate) fn configure_border(&self, window: Window) -> Result<()> {
        let values = ConfigureWindowAux::new().border_width(1);
        self.conn.configure_window(window, &values)?;
        Ok(())
    }

    pub(crate) fn configure_client(&self, window: Window, rect: Rect) -> Result<()> {
        let values = ConfigureWindowAux::new()
            .x(rect.x)
            .y(rect.y)
            .width(rect.width)
            .height(rect.height)
            .border_width(1);
        self.conn.configure_window(window, &values)?;
        Ok(())
    }

    pub(crate) fn unfocus(&self, window: Window) -> Result<()> {
        let values = ChangeWindowAttributesAux::new().border_pixel(COLOR_NORM_BORDER); 
        self.conn.change_window_attributes(window, &values)?;
        Ok(())
    }
}
