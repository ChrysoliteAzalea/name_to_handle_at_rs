#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]
//! This crate provides bindings for ```name_to_handle_at()``` and ```open_by_handle_at()``` system calls in Linux
//!
//! These system calls can be used to refer to i-nodes on the file system using a byte array that does not change during i-node lifetime
//!
//! This crate can be useful for user-space NFS servers (since NFS protocols require such references) and fanotify users wanting to refer to watched files by handles
use std::os::fd::BorrowedFd;
use std::os::fd::OwnedFd;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::convert::TryFrom;
use bitflags::bitflags;
mod ffi_bindings;
use crate::ffi_bindings::*;
use std::ffi::CStr;
mod allocate_aligned;
use crate::allocate_aligned::{AlignedBuffer,AllocationError};
use core::num::NonZero;
use std::io::ErrorKind;

/// A struct representing the file handle. The file handle itself is stored on the heap, this struct only contains a pointer to it.
#[derive(Clone)]
pub struct LinuxFileHandle
{
   v: AlignedBuffer<core::ffi::c_int>,
   mnt_id: i32,
}

bitflags!{
   /// Open flags for ```open_by_handle_at()```
   pub struct OpenFlags: u32 {
      const O_RDONLY = O_RDONLY;
      const O_WRONLY = O_WRONLY;
      const O_RDWR = O_RDWR;
      const O_CREAT = O_CREAT;
      const O_EXCL = O_EXCL;
      const O_NOCTTY = O_NOCTTY;
      const O_TRUNC = O_TRUNC;
      const O_APPEND = O_APPEND;
      const O_NONBLOCK = O_NONBLOCK;
      const O_SYNC = O_SYNC;
      const O_FSYNC = O_FSYNC;
      const O_ASYNC = O_ASYNC;
      const O_LARGEFILE = O_LARGEFILE;
      const O_DIRECTORY = O_DIRECTORY;
      const O_NOFOLLOW = O_NOFOLLOW;
      const O_CLOEXEC = O_CLOEXEC;
      const O_DIRECT = O_DIRECT;
      const O_NOATIME = O_NOATIME;
      const O_PATH = O_PATH;
      const O_TMPFILE = O_TMPFILE;
      const O_DSYNC = O_DSYNC;
      const O_RSYNC = O_RSYNC;
   }
}

impl<'a> LinuxFileHandle
{
   /// Access the bytes of file handle itself, for example, to send it to the client or save to disk
   ///
   /// The file handle should be considered an opaque value
   pub fn get_slice(&'a self) -> &'a [u8]
   {
      self.v.get()
   }
}

impl LinuxFileHandle
{
   /// Retrieve the ```mnt_id``` value from ```name_to_handle_at()``` (will return None for handles created from raw byte-arrays)
   pub fn get_mnt_id(&self) -> Option<i32>
   {
      if self.mnt_id >= 0 { Some(self.mnt_id) } else { None }
   }

   fn get_signed(s: u32) -> std::io::Result<i32>
   {
      match s.try_into()
      {
         Ok(f) => Ok(f),
         Err(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "conversion error")),
      }
   }
   
   fn get_usize<Num : TryInto<usize>>(s: Num) -> std::io::Result<usize>
   {
      match s.try_into()
      {
         Ok(f) => Ok(f),
         Err(_) => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "conversion error")),
      }
   }
   
   #[inline]
   fn take_handle_bytes(place: &[u8]) -> std::io::Result<usize>
   {
      if place.len() < core::mem::size_of::<file_handle>() { return Err(std::io::Error::from(std::io::ErrorKind::InvalidInput)); }
      let fh_ref = unsafe { &*((place.as_ptr()) as *const file_handle) };
      Self::get_usize(fh_ref.handle_bytes)
   }

   fn obtain_impl(dirfd: Option<BorrowedFd<'_>>, path: &CStr, flags: std::os::raw::c_int) -> std::io::Result<LinuxFileHandle>
   {
      let d_fd = match dirfd
      {
         Some(fd) => fd.as_raw_fd(),
         None => AT_FDCWD,
      };
      let mut mnt_id: i32 = 0;
      let mut fh: AlignedBuffer<core::ffi::c_int> = AlignedBuffer::new(NonZero::new(core::mem::size_of::<file_handle>()).ok_or(std::io::Error::from(ErrorKind::InvalidInput))?)?;
      // SAFETY: FFI function call. Validity of all arguments has been ensured earlier in this function
      let _ = unsafe { name_to_handle_at(d_fd, path.as_ptr(), fh.get_mut().as_mut_ptr() as *mut file_handle, &mut mnt_id as *mut i32, flags) };
      let first_err = std::io::Error::last_os_error(); // first call to name_to_handle_at() should normally fail with EOVERFLOW, checking if it's indeed the case
      if let Some(err) = first_err.raw_os_error()
      {
         if err != Self::get_signed(EOVERFLOW)? { return Err(first_err); } // not EOVERFLOW, that means the second call to name_to_handle_at() is pointless
      }
      else
      {
         return Err(first_err); // something very unexpected
      }
      let handle_bytes = Self::take_handle_bytes(fh.get())?;
      {
         let mut realfh: AlignedBuffer<core::ffi::c_int> = AlignedBuffer::new(NonZero::new(core::mem::size_of::<file_handle>() + Self::get_usize(handle_bytes)?).ok_or(std::io::Error::from(ErrorKind::InvalidInput))?)?;
         (&mut realfh.get_mut()[..core::mem::size_of::<file_handle>()]).copy_from_slice(fh.get());
         core::mem::swap(&mut realfh, &mut fh);
      }
      // SAFETY: FFI function call. Validity of all arguments has been ensured earlier in this function
      let r = unsafe { name_to_handle_at(d_fd, path.as_ptr(), fh.get_mut().as_mut_ptr() as *mut file_handle, &mut mnt_id as *mut i32, flags) };
      if r == 0
      {
         Ok(LinuxFileHandle { v: fh, mnt_id: mnt_id })
      }
      else
      {
         Err(std::io::Error::last_os_error())
      }
   }
   
   /// Retrieve a file handle for the given file relative to dirfd (if dirfd is None, then the current directory is used)```
   pub fn obtain(dirfd: Option<BorrowedFd<'_>>, path: &CStr) -> std::io::Result<LinuxFileHandle> { Self::obtain_impl(dirfd, path, 0) }
   
   /// Retrieve a file handle for the given file relative to dirfd (if dirfd is None, then the current directory is used), dereferencing the symbolic links```
   pub fn obtain_follow(dirfd: Option<BorrowedFd<'_>>, path: &CStr) -> std::io::Result<LinuxFileHandle> { Self::obtain_impl(dirfd, path, Self::get_signed(AT_SYMLINK_FOLLOW)?) }
   
   const EMPTY_C_STRING_STORAGE: &'static [u8] = b"\0";
   // SAFETY: EMPTY_C_STRING_STORAGE is a 0-terminated constant array
   const EMPTY_C_STRING: &'static std::ffi::CStr = unsafe { CStr::from_bytes_with_nul_unchecked(Self::EMPTY_C_STRING_STORAGE) };
   
   /// Retrieve a file handle for the file represented by a file descriptor
   pub fn obtain_fd(fd: Option<BorrowedFd<'_>>) -> std::io::Result<LinuxFileHandle> { Self::obtain_impl(fd, Self::EMPTY_C_STRING, Self::get_signed(AT_EMPTY_PATH)?) }
   
   /// Opens a file referred to by the file handle. ```mnt_fd``` should be a file descriptor for any file on the filesystem of the target file. ```flags``` is file opening flags, similar to those in ```openat()```
   /// 
   /// Please note that this function requires superuser privileges, and may not be available in containers due to security restrictions.
   ///
   /// # Safety
   ///
   /// Usage of this function may cause security issues for privileged containers, if they have some file-systems bind-mounted into them with limited visibility (i.e. only a subdirectory or a file is bind-mounted into the container, not the entire file-system). A privileged process can open a file that is not accessible by a path using ```open_by_handle_at()```, if it manages to acquire or guess its file handle. File servers operating in privileged containers that use this function should always check what the file descriptor they have acquired using this function refers to
   pub unsafe fn open_by_handle(&self, mnt_fd: BorrowedFd<'_>, flags: OpenFlags) -> std::io::Result<OwnedFd>
   {
      let f = flags.bits();
      let mut v_dup: AlignedBuffer<core::ffi::c_int> = AlignedBuffer::new(NonZero::new(self.v.get().len()).ok_or(std::io::Error::from(ErrorKind::InvalidInput))?)?;
      v_dup.get_mut().copy_from_slice(self.v.get());
      // SAFETY: FFI function call. Validity of all arguments has been ensured earlier in this function.
      let r = unsafe { open_by_handle_at(mnt_fd.as_raw_fd(), v_dup.get_mut().as_mut_ptr() as *mut file_handle, Self::get_signed(f)?) };
      if r >= 0
      {
         // SAFETY: We have confirmed that "r" is indeed a valid file descriptor (not -1), and we exclusively own it
         unsafe { Ok(OwnedFd::from_raw_fd(r)) }
      }
      else
      {
         Err(std::io::Error::last_os_error())
      }
   }
   
   /// Similar to ```clone()```, but uses fallible memory allocation API
   pub fn duplicate(&self) -> Result<LinuxFileHandle,AllocationError>
   {
      Ok(LinuxFileHandle { v: self.v.duplicate()?, mnt_id: self.mnt_id })
   }
}

impl TryFrom<&[u8]> for LinuxFileHandle
{
   type Error = AllocationError;
   
   /// Creates a file-handle from a custom byte-array
   fn try_from(value: &[u8]) -> Result<LinuxFileHandle,AllocationError>
   {
      let mut v_dup: AlignedBuffer<core::ffi::c_int> = AlignedBuffer::new(NonZero::new(value.len()).ok_or(AllocationError)?)?;
      v_dup.get_mut().copy_from_slice(value);
      Ok(LinuxFileHandle { v: v_dup, mnt_id: -1 })
   }
}
