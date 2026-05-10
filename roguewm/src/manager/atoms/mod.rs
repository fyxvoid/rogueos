use x11rb::protocol::xproto::{Atom, ConnectionExt};
use x11rb::rust_connection::RustConnection;
use anyhow::Result;

#[allow(dead_code)]
pub struct Atoms {
    pub wm_protocols: Atom,
    pub wm_delete_window: Atom,
    pub net_supported: Atom,
    pub net_client_list: Atom,
    pub net_number_of_desktops: Atom,
    pub net_current_desktop: Atom,
    pub net_active_window: Atom,
    pub net_wm_name: Atom,
    pub net_wm_state: Atom,
    pub net_wm_window_type: Atom,
    pub net_wm_window_type_dialog: Atom,
}

impl Atoms {
    pub fn new(conn: &RustConnection) -> Result<Self> {
        let wm_protocols = conn.intern_atom(false, b"WM_PROTOCOLS")?.reply()?.atom;
        let wm_delete_window = conn.intern_atom(false, b"WM_DELETE_WINDOW")?.reply()?.atom;
        
        let net_supported = conn.intern_atom(false, b"_NET_SUPPORTED")?.reply()?.atom;
        let net_client_list = conn.intern_atom(false, b"_NET_CLIENT_LIST")?.reply()?.atom;
        let net_number_of_desktops = conn.intern_atom(false, b"_NET_NUMBER_OF_DESKTOPS")?.reply()?.atom;
        let net_current_desktop = conn.intern_atom(false, b"_NET_CURRENT_DESKTOP")?.reply()?.atom;
        let net_active_window = conn.intern_atom(false, b"_NET_ACTIVE_WINDOW")?.reply()?.atom;
        let net_wm_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
        let net_wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
        let net_wm_window_type = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE")?.reply()?.atom;
        let net_wm_window_type_dialog = conn.intern_atom(false, b"_NET_WM_WINDOW_TYPE_DIALOG")?.reply()?.atom;

        Ok(Self {
            wm_protocols,
            wm_delete_window,
            net_supported,
            net_client_list,
            net_number_of_desktops,
            net_current_desktop,
            net_active_window,
            net_wm_name,
            net_wm_state,
            net_wm_window_type,
            net_wm_window_type_dialog,
        })
    }
}
