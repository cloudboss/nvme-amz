//! A Rust library to probe NVMe devices in Amazon EC2.
//!
//! It provides functionality similar to that of the `ebsnvme-id` command but adds
//! information about instance store devices, not only EBS.
//!
//! The library implements [`TryFrom<File>`] for [`Nvme`], to use as the constructor.
//!
//! # Example
//!
//! ```
//! use std::fs::File;
//!
//! use nvme_amz::Nvme;
//!
//! fn main() {
//!     let path = args().nth(1).expect("device path required");
//!     let file = File::open(path).expect("unable to open device");
//!     let nvme: Nvme = file.try_into().expect("unable to probe device");
//!     println!("{:?}", nvme);
//!     let name = nvme.name();
//!     println!("name: {}", name);
//! }
//! ```

use std::ffi::{c_char, c_uchar, c_uint, c_ulonglong, c_ushort};
#[cfg(any(feature = "ioctl-nix", feature = "ioctl-rustix"))]
use std::fs::File;
use std::os::fd::AsFd;
use std::{fmt, io};

const AMZ_EBS_MN: &str = "Amazon Elastic Block Store";
const AMZ_INST_STORE_MN: &str = "Amazon EC2 NVMe Instance Storage";
const AMZ_VENDOR_ID: c_ushort = 0x1D0F;

const NVME_ADMIN_IDENTIFY: u8 = 0x06;
const NVME_IOCTL_ADMIN_CMD_NUM: u8 = 0x41;

/// The error type for this crate.
#[derive(Debug)]
pub enum Error {
    /// Device name not found in the vendor specific field.
    DeviceNameNotFound,
    /// Wrapper for [`std::io::Error`].
    Io(io::Error),
    /// The device name could not be parsed.
    UnparseableDeviceName(String),
    /// A vendor ID other than Amazon was found.
    UnrecognizedVendorId(u16),
    /// A model other than EBS or instance store was found.
    UnrecognizedModel(String),
    /// Wrapper for [`nix::errno::Errno`].
    #[cfg(feature = "ioctl-nix")]
    NixErrno(nix::errno::Errno),
    /// Wrapper for [`rustix::io::Errno`].
    #[cfg(feature = "ioctl-rustix")]
    RustixErrno(rustix::io::Errno),
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(feature = "ioctl-nix")]
impl From<nix::errno::Errno> for Error {
    fn from(e: nix::errno::Errno) -> Self {
        Self::NixErrno(e)
    }
}

#[cfg(feature = "ioctl-rustix")]
impl From<rustix::io::Errno> for Error {
    fn from(e: rustix::io::Errno) -> Self {
        Self::RustixErrno(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::DeviceNameNotFound => write!(f, "device name not found"),
            Self::Io(e) => write!(f, "{}", e),
            Self::UnparseableDeviceName(name) => write!(f, "unparseable device name: {}", name),
            Self::UnrecognizedVendorId(id) => write!(f, "unrecognized vendor id: {}", id),
            Self::UnrecognizedModel(model) => write!(f, "unrecognized model: {}", model),
            #[cfg(feature = "ioctl-nix")]
            Self::NixErrno(e) => write!(f, "{}", e),
            #[cfg(feature = "ioctl-rustix")]
            Self::RustixErrno(e) => write!(f, "{}", e),
        }
    }
}

type Result<T> = std::result::Result<T, Error>;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct NvmeIdPsd {
    mp: c_ushort,
    rsvd2: c_uchar,
    flags: c_uchar,
    enlat: c_uint,
    exlat: c_uint,
    rrt: c_uchar,
    rrl: c_uchar,
    rwt: c_uchar,
    rwl: c_uchar,
    idlp: c_ushort,
    ips: c_uchar,
    rsvd19: c_uchar,
    actp: c_ushort,
    apws: c_uchar,
    rsvd23: [c_uchar; 9],
}

impl Default for NvmeIdPsd {
    fn default() -> Self {
        Self {
            mp: 0,
            rsvd2: 0,
            flags: 0,
            enlat: 0,
            exlat: 0,
            rrt: 0,
            rrl: 0,
            rwt: 0,
            rwl: 0,
            idlp: 0,
            ips: 0,
            rsvd19: 0,
            actp: 0,
            apws: 0,
            rsvd23: [0; 9],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct NvmeVuIdCtrlField {
    bdev: [c_uchar; 32],
    reserved0: [c_uchar; 992],
}

impl Default for NvmeVuIdCtrlField {
    fn default() -> Self {
        Self {
            bdev: [0; 32],
            reserved0: [0; 992],
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct NvmeIdCtrl {
    vid: c_ushort,
    ssvid: c_ushort,
    sn: [c_char; 20],
    mn: [c_char; 40],
    fr: [c_char; 8],
    rab: c_uchar,
    ieee: [c_uchar; 3],
    cmic: c_uchar,
    mdts: c_uchar,
    cntlid: c_ushort,
    ver: c_uint,
    rtd3r: c_uint,
    rtd3e: c_uint,
    oaes: c_uint,
    ctratt: c_uint,
    rrls: c_ushort,
    rsvd102: [c_uchar; 9],
    cntrltype: c_uchar,
    fguid: [c_uchar; 16],
    crdt1: c_ushort,
    crdt2: c_ushort,
    crdt3: c_ushort,
    rsvd134: [c_uchar; 119],
    nvmsr: c_uchar,
    vwci: c_uchar,
    mec: c_uchar,
    oacs: c_ushort,
    acl: c_uchar,
    aerl: c_uchar,
    frmw: c_uchar,
    lpa: c_uchar,
    elpe: c_uchar,
    npss: c_uchar,
    avscc: c_uchar,
    apsta: c_uchar,
    wctemp: c_ushort,
    cctemp: c_ushort,
    mtfa: c_ushort,
    hmpre: c_uint,
    hmmin: c_uint,
    tnvmcap: [c_uchar; 16],
    unvmcap: [c_uchar; 16],
    rpmbs: c_uint,
    edstt: c_ushort,
    dsto: c_uchar,
    fwug: c_uchar,
    kas: c_ushort,
    hctma: c_ushort,
    mntmt: c_ushort,
    mxtmt: c_ushort,
    sanicap: c_uint,
    hmminds: c_uint,
    hmmaxd: c_ushort,
    nsetidmax: c_ushort,
    endgidmax: c_ushort,
    anatt: c_uchar,
    anacap: c_uchar,
    anagrpmax: c_uint,
    nanagrpid: c_uint,
    pels: c_uint,
    domainid: c_ushort,
    rsvd358: [c_uchar; 10],
    megcap: [c_uchar; 16],
    tmpthha: c_uchar,
    rsvd385: [c_uchar; 127],
    sqes: c_uchar,
    cqes: c_uchar,
    maxcmd: c_ushort,
    nn: c_uint,
    oncs: c_ushort,
    fuses: c_ushort,
    fna: c_uchar,
    vwc: c_uchar,
    awun: c_ushort,
    awupf: c_ushort,
    icsvscc: c_uchar,
    nwpc: c_uchar,
    acwu: c_ushort,
    ocfs: c_ushort,
    sgls: c_uint,
    mnan: c_uint,
    maxdna: [c_uchar; 16],
    maxcna: c_uint,
    oaqd: c_uint,
    rsvd568: [c_uchar; 200],
    subnqn: [c_char; 256],
    rsvd1024: [c_uchar; 768],
    ioccsz: c_uint,
    iorcsz: c_uint,
    icdoff: c_ushort,
    fcatt: c_uchar,
    msdbd: c_uchar,
    ofcs: c_ushort,
    dctype: c_uchar,
    rsvd1807: [c_uchar; 241],
    psd: [NvmeIdPsd; 32],
    vs: NvmeVuIdCtrlField,
}

impl Default for NvmeIdCtrl {
    fn default() -> Self {
        Self {
            vid: 0,
            ssvid: 0,
            sn: [0; 20],
            mn: [0; 40],
            fr: [0; 8],
            rab: 0,
            ieee: [0; 3],
            cmic: 0,
            mdts: 0,
            cntlid: 0,
            ver: 0,
            rtd3r: 0,
            rtd3e: 0,
            oaes: 0,
            ctratt: 0,
            rrls: 0,
            rsvd102: [0; 9],
            cntrltype: 0,
            fguid: [0; 16],
            crdt1: 0,
            crdt2: 0,
            crdt3: 0,
            rsvd134: [0; 119],
            nvmsr: 0,
            vwci: 0,
            mec: 0,
            oacs: 0,
            acl: 0,
            aerl: 0,
            frmw: 0,
            lpa: 0,
            elpe: 0,
            npss: 0,
            avscc: 0,
            apsta: 0,
            wctemp: 0,
            cctemp: 0,
            mtfa: 0,
            hmpre: 0,
            hmmin: 0,
            tnvmcap: [0; 16],
            unvmcap: [0; 16],
            rpmbs: 0,
            edstt: 0,
            dsto: 0,
            fwug: 0,
            kas: 0,
            hctma: 0,
            mntmt: 0,
            mxtmt: 0,
            sanicap: 0,
            hmminds: 0,
            hmmaxd: 0,
            nsetidmax: 0,
            endgidmax: 0,
            anatt: 0,
            anacap: 0,
            anagrpmax: 0,
            nanagrpid: 0,
            pels: 0,
            domainid: 0,
            rsvd358: [0; 10],
            megcap: [0; 16],
            tmpthha: 0,
            rsvd385: [0; 127],
            sqes: 0,
            cqes: 0,
            maxcmd: 0,
            nn: 0,
            oncs: 0,
            fuses: 0,
            fna: 0,
            vwc: 0,
            awun: 0,
            awupf: 0,
            icsvscc: 0,
            nwpc: 0,
            acwu: 0,
            ocfs: 0,
            sgls: 0,
            mnan: 0,
            maxdna: [0; 16],
            maxcna: 0,
            oaqd: 0,
            rsvd568: [0; 200],
            subnqn: [0; 256],
            rsvd1024: [0; 768],
            ioccsz: 0,
            iorcsz: 0,
            icdoff: 0,
            fcatt: 0,
            msdbd: 0,
            ofcs: 0,
            dctype: 0,
            rsvd1807: [0; 241],
            psd: [NvmeIdPsd::default(); 32],
            vs: NvmeVuIdCtrlField::default(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct NvmePassthruCmd {
    opcode: c_uchar,
    flags: c_uchar,
    rsvd1: c_ushort,
    nsid: c_uint,
    cdw2: c_uint,
    cdw3: c_uint,
    metadata: c_ulonglong,
    addr: c_ulonglong,
    metadata_len: c_uint,
    data_len: c_uint,
    cdw10: c_uint,
    cdw11: c_uint,
    cdw12: c_uint,
    cdw13: c_uint,
    cdw14: c_uint,
    cdw15: c_uint,
    timeout_ms: c_uint,
    result: c_uint,
}

type NvmeAdminCmd = NvmePassthruCmd;

#[cfg(feature = "ioctl-nix")]
mod ioctl_nix {
    use std::os::fd::{AsFd, AsRawFd};

    use nix::ioctl_readwrite;

    use super::*;

    pub(super) fn nvme_identify_ctrl<F: AsFd>(fd: F) -> Result<NvmeIdCtrl> {
        ioctl_readwrite!(
            nvme_identify_ctrl_inner,
            b'N',
            NVME_IOCTL_ADMIN_CMD_NUM,
            NvmeAdminCmd
        );
        let mut out = NvmeIdCtrl::default();
        let out_ptr = &mut out as *mut _;
        let mut nvme_admin_cmd = NvmeAdminCmd {
            addr: out_ptr as c_ulonglong,
            cdw10: 1,
            data_len: std::mem::size_of::<NvmeIdCtrl>() as c_uint,
            opcode: NVME_ADMIN_IDENTIFY,
            ..Default::default()
        };
        let nvme_admin_cmd_ptr = &mut nvme_admin_cmd as *mut _;
        unsafe { nvme_identify_ctrl_inner(fd.as_fd().as_raw_fd(), nvme_admin_cmd_ptr) }?;
        Ok(out)
    }
}

#[cfg(feature = "ioctl-rustix")]
mod ioctl_rustix {
    use std::ffi::c_void;
    use std::os::fd::AsFd;

    use rustix::io;
    use rustix::ioctl::{ioctl, Direction, Ioctl, IoctlOutput, Opcode};

    use super::*;

    unsafe impl Ioctl for NvmeAdminCmd {
        type Output = NvmeIdCtrl;

        const IS_MUTATING: bool = false;
        const OPCODE: Opcode = Opcode::from_components(
            Direction::ReadWrite,
            b'N',
            NVME_IOCTL_ADMIN_CMD_NUM,
            std::mem::size_of::<NvmeAdminCmd>(),
        );

        fn as_ptr(&mut self) -> *mut c_void {
            self as *const _ as *mut _
        }

        unsafe fn output_from_ptr(ret: IoctlOutput, ptr: *mut c_void) -> io::Result<Self::Output> {
            if ret != 0 {
                return Err(io::Errno::from_raw_os_error(ret));
            }
            let sellf = ptr.cast::<NvmeAdminCmd>().read();
            let data_ptr = sellf.addr as *const NvmeIdCtrl;
            let output = data_ptr.cast::<NvmeIdCtrl>().read();
            Ok(output)
        }
    }

    pub(super) fn nvme_identify_ctrl<F: AsFd>(fd: F) -> Result<NvmeIdCtrl> {
        let mut data = NvmeIdCtrl::default();
        let nvme_admin_cmd = NvmeAdminCmd {
            addr: &mut data as *mut _ as c_ulonglong,
            cdw10: 1,
            data_len: std::mem::size_of::<NvmeIdCtrl>() as c_uint,
            opcode: NVME_ADMIN_IDENTIFY,
            ..Default::default()
        };
        let output = unsafe { ioctl(fd, nvme_admin_cmd) }?;
        Ok(output)
    }
}

/// A structure containing vendor-specific device names.
pub struct Names {
    /// Device name defined in the block device mapping.
    pub device_name: Option<String>,
    /// Virtual name for instance store volumes, such as ephemeral0.
    pub virtual_name: Option<String>,

    // Force internal creation so the name() method cannot panic, by ensuring
    // either device_name or virtual_name have Some(value).
    _internal: (),
}

impl fmt::Debug for Names {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Names")
            .field("device_name", &self.device_name)
            .field("virtual_name", &self.virtual_name)
            .finish()
    }
}

impl TryFrom<&[c_uchar]> for Names {
    type Error = Error;

    fn try_from(chars: &[u8]) -> Result<Self> {
        const COLON: u8 = 0x3a;
        const SPACE: u8 = 0x20;
        const NULL: u8 = 0x0;

        // For instance store volumes, the value of chars is delimited by a colon,
        // and contains <virtual_name>:<device_name> when the device is defined in
        // the block device mapping. When the device is not defined in the block
        // device mapping, the value is <virtual_name>:none. The virtual name is
        // automatically defined as ephemeral<n>, where <n> is incremented for each
        // attached volume.
        // For EBS volumes, which must have a device name defined when attached to
        // an instance, the value has no virtual name and no delimiter. The single
        // value corresponds to the device name.

        let mut field1 = String::new();
        let mut field2 = String::new();

        let mut has_delim = false;

        for c in chars.iter() {
            if *c == NULL || *c == SPACE {
                break;
            }
            if *c == COLON {
                has_delim = true;
                continue;
            }
            if !has_delim {
                field1.push(*c as char);
            } else {
                field2.push(*c as char);
            }
        }

        if field1.starts_with("/dev/") {
            field1 = field1[5..].into();
        }

        if field2.starts_with("/dev/") {
            field2 = field2[5..].into();
        }

        let device_name = if has_delim {
            if field2 == "none" {
                None
            } else {
                Some(&field2)
            }
        } else {
            Some(&field1)
        };

        let virtual_name = if has_delim { Some(&field1) } else { None };

        if device_name.is_none() && virtual_name.is_none() {
            return Err(Error::UnparseableDeviceName(
                chars.iter().map(|c| *c as char).collect(),
            ));
        }

        Ok(Self {
            device_name: device_name.cloned(),
            virtual_name: virtual_name.cloned(),
            _internal: (),
        })
    }
}

/// The model of the NVMe device.
#[derive(Debug)]
pub enum Model {
    /// Elastic Block Store volume.
    AmazonElasticBlockStore,
    /// Instance store volume.
    AmazonInstanceStore,
}

/// The vendor ID of the NVMe device.
#[derive(Debug)]
pub struct VendorId(pub u16);

/// An NVMe device, containing a subset of all identifying information.
#[derive(Debug)]
pub struct Nvme {
    /// The [model](Model) of the device.
    pub model: Model,
    /// The [structure](Names) containing vendor-specific device names.
    pub names: Names,
    /// The [vendor ID](VendorId) of the device.
    pub vendor_id: VendorId,
}

impl Nvme {
    /// Get the vendor specific device name or fall back to the virtual name if it
    /// is an instance store volume.
    pub fn name(&self) -> &str {
        self.names
            .device_name
            .as_ref()
            .unwrap_or_else(|| self.names.virtual_name.as_ref().unwrap())
    }

    fn from_fd<F, IoctlFn>(fd: F, f: IoctlFn) -> Result<Self>
    where
        F: AsFd,
        IoctlFn: FnOnce(F) -> Result<NvmeIdCtrl>,
    {
        let ctrl = f(fd)?;
        if ctrl.vid != AMZ_VENDOR_ID {
            return Err(Error::UnrecognizedVendorId(ctrl.vid));
        }
        let mut model_str = String::from_iter(ctrl.mn.map(|c| c as u8 as char));
        model_str.truncate(model_str.trim_end().len());
        let model = match model_str.as_str() {
            AMZ_EBS_MN => Model::AmazonElasticBlockStore,
            AMZ_INST_STORE_MN => Model::AmazonInstanceStore,
            _ => return Err(Error::UnrecognizedModel(model_str)),
        };
        let names = ctrl.vs.bdev.as_slice().try_into()?;
        Ok(Self {
            model,
            names,
            vendor_id: VendorId(ctrl.vid),
        })
    }
}

#[cfg(any(feature = "ioctl-nix", feature = "ioctl-rustix"))]
impl TryFrom<File> for Nvme {
    type Error = Error;

    #[cfg(feature = "ioctl-nix")]
    fn try_from(f: File) -> Result<Self> {
        Self::from_fd(f.as_fd(), ioctl_nix::nvme_identify_ctrl)
    }

    #[cfg(feature = "ioctl-rustix")]
    fn try_from(f: File) -> Result<Self> {
        Self::from_fd(f.as_fd(), ioctl_rustix::nvme_identify_ctrl)
    }
}

#[cfg(all(feature = "ioctl-nix", feature = "ioctl-rustix"))]
compile_error!("The features ioctl-nix and ioctl-rustix are mutually exclusive");
