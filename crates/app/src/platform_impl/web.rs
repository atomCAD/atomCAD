// This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of
// the MPL was not distributed with this file, You can obtain one at <http://mozilla.org/MPL/2.0/>.

/// Register the panic hook that logs errors to the Javascript console.
pub(crate) fn setup_panic_handler() {
    console_error_panic_hook::set_once();
}

// End of File
