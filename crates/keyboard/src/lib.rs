// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

bitflags::bitflags! {
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct ModifierKeys: u8 {
        const CAPSLOCK = 1 << 0;
        const SHIFT    = 1 << 1;
        const CONTROL  = 1 << 2;
        const OPTION   = 1 << 3;
        const COMMAND  = 1 << 4;
        const NUMPAD   = 1 << 5;
        const HELP     = 1 << 6;
        const FUNCTION = 1 << 7;
    }
}

// End of File
