// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::process::Command;

use anyhow::{bail, Result};

use crate::util::*;

pub(crate) fn cmd_clippy(strict: bool, quiet: bool) -> Result<()> {
    let wroot = workspace_root()?;

    let run_clippy = |args: &[&str]| -> Result<bool> {
        let mut cmd = Command::new("cargo");
        cmd.arg("clippy").args(args).current_dir(&wroot);

        if quiet {
            cmd.arg("--quiet");
        }

        // no-deps and subsequent options must follow `--`
        cmd.args(["--", "--no-deps"]);

        // Disable lossless cast warnings until
        // https://github.com/oxidecomputer/usdt/issues/240 is fixed.
        // cmd.args(["--warn", "clippy::cast_lossless"]);

        if strict {
            cmd.arg("-Dwarnings");
        }

        let status = cmd.spawn()?.wait()?;
        Ok(!status.success())
    };

    let mut failed = false;

    // Everything in the workspace (including tests, etc)
    failed |= run_clippy(&["--workspace", "--all-targets"])?;

    if failed {
        bail!("Clippy failure(s) detected")
    }

    Ok(())
}
