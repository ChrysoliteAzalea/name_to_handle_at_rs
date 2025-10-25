use std::os::fd::AsFd;
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use name_to_handle_at_rs::LinuxFileHandle;
use name_to_handle_at_rs::OpenFlags;
use std::ffi::CString;
use core::num::NonZero;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
      // This test will fail if CAP_DAC_READ_SEARCH is not effective for it
      // This test will also fail in unprivileged containers
        let path = CString::new("/bin/sh").unwrap();
        let fh = LinuxFileHandle::obtain_follow(None, &path).unwrap(); // trying to choose a file that exists on most systems
        let f_obj = std::fs::File::open("/bin/sh").unwrap();
        let fd = unsafe { fh.open_by_handle(f_obj.as_fd(), OpenFlags::O_PATH).unwrap() };
    }
    
    #[test]
    fn fd_works() {
      // This test checks that the file descriptor opened by file handle points to the same i-node
       let fd_obj = std::fs::File::open("/bin/sh").unwrap();
       let fh = LinuxFileHandle::obtain_fd(Some(fd_obj.as_fd())).unwrap();
       let owned_fd = unsafe { fh.open_by_handle(fd_obj.as_fd(), OpenFlags::O_PATH).unwrap() };
       let mut original = MaybeUninit::<libc::stat>::uninit();
       let mut opened = MaybeUninit::<libc::stat>::uninit();
       assert_eq!(unsafe { libc::fstat(fd_obj.as_raw_fd(), original.as_mut_ptr()) }, 0);
       assert_eq!(unsafe { libc::fstat(owned_fd.as_raw_fd(), opened.as_mut_ptr()) }, 0);
       unsafe { assert_eq!(original.assume_init().st_ino, opened.assume_init().st_ino) };
    } // */
    
    /*#[test]
    fn aligned_memory()
    {
      // This test is commented-out, because it requires the "allocate_aligned" module to be public
      let mut mem = name_to_handle_at_rs::allocate_aligned::AlignedBuffer::<core::ffi::c_int>::new(NonZero::new(4).unwrap()).unwrap();
      mem.get_mut().copy_from_slice("test".as_bytes());
      let second = mem.duplicate().unwrap();
    } */
}