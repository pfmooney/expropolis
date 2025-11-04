// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::os::unix::fs::FileTypeExt;
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Context;
use cpuid_utils::CpuidSet;
use propolis_types::CpuidIdent;
use propolis_types::CpuidValues;
use propolis_types::CpuidVendor;
use serde::{Deserialize, Serialize};

use cpuid_profile_config::*;
use propolis::block;
use propolis::hw::pci::Bdf;

use crate::cidata::build_cidata_be;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    pub main: Main,

    #[serde(default, rename = "dev")]
    pub devices: BTreeMap<String, Device>,

    #[serde(default, rename = "block_dev")]
    pub block_devs: BTreeMap<String, BlockDevice>,

    #[serde(default, rename = "cpuid")]
    pub cpuid_profiles: BTreeMap<String, CpuidProfile>,

    pub cloudinit: Option<CloudInit>,
}
impl Config {
    pub fn cpuid_profile(&self) -> Option<&CpuidProfile> {
        match self.main.cpuid_profile.as_ref() {
            Some(name) => self.cpuid_profiles.get(name),
            None => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Main {
    pub name: String,
    pub cpus: u8,
    pub bootrom: String,
    pub bootrom_version: Option<String>,
    pub memory: usize,
    pub use_reservoir: Option<bool>,
    pub cpuid_profile: Option<String>,
    /// Process exitcode to emit if/when instance halts
    ///
    /// Default: 0
    #[serde(default)]
    pub exit_on_halt: u8,
    /// Process exitcode to emit if/when instance reboots
    ///
    /// Default: None, does not exit on reboot
    #[serde(default)]
    pub exit_on_reboot: Option<u8>,

    /// Request bootrom override boot order using the devices specified
    pub boot_order: Option<Vec<String>>,
}

/// A hard-coded device, either enabled by default or accessible locally
/// on a machine.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Device {
    pub driver: String,

    #[serde(flatten, default)]
    pub options: BTreeMap<String, toml::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockOpts {
    pub block_size: Option<u32>,
    pub read_only: Option<bool>,
    pub skip_flush: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockDevice {
    #[serde(default, rename = "type")]
    pub bdtype: String,

    #[serde(flatten)]
    pub block_opts: BlockOpts,

    #[serde(flatten, default)]
    pub options: BTreeMap<String, toml::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CloudInit {
    pub user_data: Option<String>,
    pub meta_data: Option<String>,
    pub network_config: Option<String>,

    // allow path-style contents as well
    pub user_data_path: Option<String>,
    pub meta_data_path: Option<String>,
    pub network_config_path: Option<String>,
}

#[derive(Deserialize)]
struct FileConfig {
    path: String,
    workers: Option<NonZeroUsize>,
}
#[derive(Deserialize)]
struct MemAsyncConfig {
    size: u64,
    workers: Option<usize>,
}

#[derive(Deserialize)]
pub struct VionaDeviceParams {
    tx_copy_data: Option<bool>,
    tx_header_pad: Option<u16>,
}
impl VionaDeviceParams {
    pub fn from_opts(
        opts: &BTreeMap<String, toml::Value>,
    ) -> Result<Option<propolis::hw::virtio::viona::DeviceParams>, anyhow::Error>
    {
        use propolis::hw::virtio::viona::DeviceParams;
        let parsed: Self = opt_deser(opts)?;
        let out = if parsed.tx_copy_data.is_some()
            || parsed.tx_header_pad.is_some()
        {
            let default = DeviceParams::default();

            Some(DeviceParams {
                copy_data: parsed.tx_copy_data.unwrap_or(default.copy_data),
                header_pad: parsed.tx_header_pad.unwrap_or(default.header_pad),
            })
        } else {
            None
        };
        Ok(out)
    }
}

// Try to turn unmatched flattened options into a config struct
fn opt_deser<'de, T: Deserialize<'de>>(
    value: &BTreeMap<String, toml::Value>,
) -> Result<T, anyhow::Error> {
    let map = toml::map::Map::from_iter(value.clone());
    let config = map.try_into::<T>()?;
    Ok(config)
}

const DEFAULT_WORKER_COUNT: usize = 8;
const MAX_FILE_WORKERS: usize = 32;

pub fn block_backend(
    config: &Config,
    dev: &Device,
    log: &slog::Logger,
) -> (Arc<dyn block::Backend>, String) {
    let backend_name = dev.options.get("block_dev").unwrap().as_str().unwrap();
    let Some(be) = config.block_devs.get(backend_name) else {
        panic!("No configured block device named \"{backend_name}\"");
    };
    let opts = block::BackendOpts {
        block_size: be.block_opts.block_size,
        read_only: be.block_opts.read_only,
        skip_flush: be.block_opts.skip_flush,
    };

    let be: Arc<dyn block::Backend> = match &be.bdtype as &str {
        "file" => {
            let parsed: FileConfig = opt_deser(&be.options).unwrap();

            // Check if raw device is being used and gripe if it isn't
            let meta = std::fs::metadata(&parsed.path)
                .with_context(|| {
                    format!(
                        "opening {} for block device \"{backend_name}\"",
                        parsed.path,
                    )
                })
                .expect("file device path is valid");

            if meta.file_type().is_block_device() {
                slog::warn!(log, "Block backend using standard device rather than raw";
                    "path" => &parsed.path);
            }

            let workers: NonZeroUsize = match parsed.workers {
                Some(workers) => {
                    if workers.get() <= MAX_FILE_WORKERS {
                        workers
                    } else {
                        slog::warn!(
                            log,
                            "workers must be between 1 and {} \
                            Using default value of {}.",
                            MAX_FILE_WORKERS,
                            DEFAULT_WORKER_COUNT,
                        );
                        NonZeroUsize::new(DEFAULT_WORKER_COUNT).unwrap()
                    }
                }
                None => NonZeroUsize::new(DEFAULT_WORKER_COUNT).unwrap(),
            };
            block::FileBackend::create(&parsed.path, opts, workers).unwrap()
        }
        "mem-async" => {
            let parsed: MemAsyncConfig = opt_deser(&be.options).unwrap();

            block::MemAsyncBackend::create(
                parsed.size,
                opts,
                NonZeroUsize::new(
                    parsed.workers.unwrap_or(DEFAULT_WORKER_COUNT),
                )
                .unwrap(),
            )
            .unwrap()
        }
        "cloudinit" => build_cidata_be(config).unwrap(),
        _ => {
            panic!("unrecognized block dev type {}!", be.bdtype);
        }
    };
    (be, backend_name.into())
}

pub fn parse(path: &str) -> anyhow::Result<Config> {
    let file_data =
        std::fs::read(path).context("Failed to read given config.toml")?;
    Ok(toml::from_str::<Config>(
        std::str::from_utf8(&file_data)
            .context("config should be valid utf-8")?,
    )?)
}

pub fn parse_bdf(v: &str) -> Option<Bdf> {
    let mut fields = Vec::with_capacity(3);
    for f in v.split('.') {
        let num = usize::from_str(f).ok()?;
        if num > u8::MAX as usize {
            return None;
        }
        fields.push(num as u8);
    }

    if fields.len() == 3 {
        Bdf::new(fields[0], fields[1], fields[2])
    } else {
        None
    }
}

pub fn parse_cpuid(config: &Config) -> anyhow::Result<Option<CpuidSet>> {
    if let Some(profile) = config.cpuid_profile() {
        let vendor = match profile.vendor {
            CpuVendor::Amd => CpuidVendor::Amd,
            CpuVendor::Intel => CpuidVendor::Intel,
        };
        let mut set = CpuidSet::new(vendor);
        let entries: Vec<CpuidEntry> = profile.try_into()?;
        for entry in entries {
            let conflict = set.insert(
                CpuidIdent { leaf: entry.func, subleaf: entry.idx },
                CpuidValues::from(entry.values),
            )?;

            if conflict.is_some() {
                anyhow::bail!(
                    "conflicing entry at func:{:#?} idx:{:#?}",
                    entry.func,
                    entry.idx
                )
            }
        }
        Ok(Some(set))
    } else {
        Ok(None)
    }
}
