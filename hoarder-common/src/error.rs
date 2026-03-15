use core::fmt::{self, Debug};

pub type Result<T> = core::result::Result<T, HoarderError>;

#[derive(Debug)]
pub enum HoarderError {
    PushError,
    MemAllocFail,
    BufferAllocFail,
    IoError(i32),
}

impl fmt::Display for HoarderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PushError => write!(f, "failed to push entry to queue"),
            Self::MemAllocFail => write!(f, "failed to allocate memory"),
            Self::BufferAllocFail => write!(f, "failed to allocate buffer"),
            Self::IoError(errno) => write!(f, "os error: {}", errno),
        }
    }
}

impl From<i32> for HoarderError {
    fn from(value: i32) -> Self {
        Self::IoError(value)
    }
}

impl From<Errno> for HoarderError {
    fn from(value: Errno) -> Self {
        Self::IoError(value.0)
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Errno(pub i32);

#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn __errno_location() -> *mut i32;
}

#[cfg(target_os = "linux")]
impl Errno {
    #[inline]
    pub fn last() -> Self {
        unsafe { Self(*__errno_location()) }
    }

    #[inline]
    pub fn set(errno: Self) {
        unsafe {
            *__errno_location() = errno.0;
        }
    }

    /// from_raw_syscall_error takes a NEGATIVE value just like
    /// linux syscall would return in case of an error and it
    /// returns the corresponding Errno value
    pub fn from_raw_syscall_error(val: i32) -> Self {
        Errno((-val) as i32)
    }
}

impl Errno {
    pub const EPERM: Self = Self(1);
    pub const ENOENT: Self = Self(2);
    pub const ESRCH: Self = Self(3);
    pub const EINTR: Self = Self(4);
    pub const EIO: Self = Self(5);
    pub const ENXIO: Self = Self(6);
    pub const E2BIG: Self = Self(7);
    pub const ENOEXEC: Self = Self(8);
    pub const EBADF: Self = Self(9);
    pub const ECHILD: Self = Self(10);
    pub const EAGAIN: Self = Self(11);
    pub const EWOULDBLOCK: Self = Self(11);
    pub const ENOMEM: Self = Self(12);
    pub const EACCES: Self = Self(13);
    pub const EFAULT: Self = Self(14);
    pub const ENOTBLK: Self = Self(15);
    pub const EBUSY: Self = Self(16);
    pub const EEXIST: Self = Self(17);
    pub const EXDEV: Self = Self(18);
    pub const ENODEV: Self = Self(19);
    pub const ENOTDIR: Self = Self(20);
    pub const EISDIR: Self = Self(21);
    pub const EINVAL: Self = Self(22);
    pub const ENFILE: Self = Self(23);
    pub const EMFILE: Self = Self(24);
    pub const ENOTTY: Self = Self(25);
    pub const ETXTBSY: Self = Self(26);
    pub const EFBIG: Self = Self(27);
    pub const ENOSPC: Self = Self(28);
    pub const ESPIPE: Self = Self(29);
    pub const EROFS: Self = Self(30);
    pub const EMLINK: Self = Self(31);
    pub const EPIPE: Self = Self(32);
    pub const EDOM: Self = Self(33);
    pub const ERANGE: Self = Self(34);

    pub const EDEADLK: Self = Self(35);
    pub const EDEADLOCK: Self = Self(35);
    pub const ENAMETOOLONG: Self = Self(36);
    pub const ENOLCK: Self = Self(37);
    pub const ENOSYS: Self = Self(38);
    pub const ENOTEMPTY: Self = Self(39);
    pub const ELOOP: Self = Self(40);

    pub const ENOMSG: Self = Self(42);
    pub const EIDRM: Self = Self(43);
    pub const ECHRNG: Self = Self(44);
    pub const EL2NSYNC: Self = Self(45);
    pub const EL3HLT: Self = Self(46);
    pub const EL3RST: Self = Self(47);
    pub const ELNRNG: Self = Self(48);
    pub const EUNATCH: Self = Self(49);
    pub const ENOCSI: Self = Self(50);
    pub const EL2HLT: Self = Self(51);
    pub const EBADE: Self = Self(52);
    pub const EBADR: Self = Self(53);
    pub const EXFULL: Self = Self(54);
    pub const ENOANO: Self = Self(55);
    pub const EBADRQC: Self = Self(56);
    pub const EBADSLT: Self = Self(57);

    pub const EBFONT: Self = Self(59);
    pub const ENOSTR: Self = Self(60);
    pub const ENODATA: Self = Self(61);
    pub const ETIME: Self = Self(62);
    pub const ENOSR: Self = Self(63);
    pub const ENONET: Self = Self(64);
    pub const ENOPKG: Self = Self(65);
    pub const EREMOTE: Self = Self(66);
    pub const ENOLINK: Self = Self(67);
    pub const EADV: Self = Self(68);
    pub const ESRMNT: Self = Self(69);
    pub const ECOMM: Self = Self(70);
    pub const EPROTO: Self = Self(71);
    pub const EMULTIHOP: Self = Self(72);
    pub const EDOTDOT: Self = Self(73);
    pub const EBADMSG: Self = Self(74);

    pub const EOVERFLOW: Self = Self(75);
    pub const ENOTUNIQ: Self = Self(76);
    pub const EBADFD: Self = Self(77);
    pub const EREMCHG: Self = Self(78);
    pub const ELIBACC: Self = Self(79);
    pub const ELIBBAD: Self = Self(80);
    pub const ELIBSCN: Self = Self(81);
    pub const ELIBMAX: Self = Self(82);
    pub const ELIBEXEC: Self = Self(83);
    pub const EILSEQ: Self = Self(84);
    pub const ERESTART: Self = Self(85);
    pub const ESTRPIPE: Self = Self(86);
    pub const EUSERS: Self = Self(87);

    pub const ENOTSOCK: Self = Self(88);
    pub const EDESTADDRREQ: Self = Self(89);
    pub const EMSGSIZE: Self = Self(90);
    pub const EPROTOTYPE: Self = Self(91);
    pub const ENOPROTOOPT: Self = Self(92);
    pub const EPROTONOSUPPORT: Self = Self(93);
    pub const ESOCKTNOSUPPORT: Self = Self(94);
    pub const EOPNOTSUPP: Self = Self(95);
    pub const EPFNOSUPPORT: Self = Self(96);
    pub const EAFNOSUPPORT: Self = Self(97);
    pub const EADDRINUSE: Self = Self(98);
    pub const EADDRNOTAVAIL: Self = Self(99);
    pub const ENETDOWN: Self = Self(100);
    pub const ENETUNREACH: Self = Self(101);
    pub const ENETRESET: Self = Self(102);
    pub const ECONNABORTED: Self = Self(103);
    pub const ECONNRESET: Self = Self(104);
    pub const ENOBUFS: Self = Self(105);
    pub const EISCONN: Self = Self(106);
    pub const ENOTCONN: Self = Self(107);
    pub const ESHUTDOWN: Self = Self(108);
    pub const ETOOMANYREFS: Self = Self(109);
    pub const ETIMEDOUT: Self = Self(110);
    pub const ECONNREFUSED: Self = Self(111);
    pub const EHOSTDOWN: Self = Self(112);
    pub const EHOSTUNREACH: Self = Self(113);
    pub const EALREADY: Self = Self(114);
    pub const EINPROGRESS: Self = Self(115);

    pub const ESTALE: Self = Self(116);
    pub const EUCLEAN: Self = Self(117);
    pub const ENOTNAM: Self = Self(118);
    pub const ENAVAIL: Self = Self(119);
    pub const EISNAM: Self = Self(120);
    pub const EREMOTEIO: Self = Self(121);
    pub const EDQUOT: Self = Self(122);
    pub const ENOMEDIUM: Self = Self(123);
    pub const EMEDIUMTYPE: Self = Self(124);
    pub const ECANCELED: Self = Self(125);
    pub const ENOKEY: Self = Self(126);
    pub const EKEYEXPIRED: Self = Self(127);
    pub const EKEYREVOKED: Self = Self(128);
    pub const EKEYREJECTED: Self = Self(129);
    pub const EOWNERDEAD: Self = Self(130);
    pub const ENOTRECOVERABLE: Self = Self(131);
    pub const ERFKILL: Self = Self(132);
    pub const EHWPOISON: Self = Self(133);
}

impl From<i32> for Errno {
    #[inline]
    fn from(v: i32) -> Self {
        Self(v)
    }
}

impl From<Errno> for i32 {
    #[inline]
    fn from(e: Errno) -> i32 {
        e.0
    }
}
