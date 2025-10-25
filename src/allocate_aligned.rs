use std::alloc::{alloc_zeroed,Layout};
use core::num::NonZero;
use core::marker::PhantomData;
use core::fmt::{Debug,Display,Formatter};
use core::ops::{Deref,DerefMut};
use core::mem::MaybeUninit;

pub struct AllocationError;

impl Debug for AllocationError
{
   #[inline]
   fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result
   {
      f.write_str("Cannot allocate memory")
   }
}

impl Display for AllocationError
{
   #[inline]
   fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result
   {
      <Self as Debug>::fmt(self, f)
   }
}

impl core::error::Error for AllocationError {}

impl From<AllocationError> for std::io::Error
{
   #[inline]
   fn from(_: AllocationError) -> Self
   {
      std::io::Error::from(std::io::ErrorKind::OutOfMemory)
   }
}

pub type AllocResult<T> = Result<T,AllocationError>;

pub(crate) struct AlignedBuffer<AlignAs>
{
   ptr: MaybeUninit<Box<[u8]>>, // explanation: I tried to use NonNull<u8> first, but it resulted in Miri errors related to stacked borrows, however, Box passes Miri tests
   p: PhantomData<AlignAs>,
}

impl<AlignAs> AlignedBuffer<AlignAs>
{
   #[inline]
   pub(crate) fn new(array_size: NonZero<usize>) -> AllocResult<AlignedBuffer<AlignAs>>
   {
      let layout = Layout::from_size_align(array_size.get(), core::mem::align_of::<AlignAs>()).or(Err(AllocationError))?;
      // SAFETY: layout is checked to contain non-zero values
      let memory = unsafe { alloc_zeroed(layout) };
      if memory.is_null() { return Err(AllocationError); }
      let sliceptr = core::ptr::slice_from_raw_parts_mut::<u8>(memory, array_size.get());
      // SAFETY: sliceptr is a newly-allocated memory block from the global allocator
      Ok(AlignedBuffer { ptr: MaybeUninit::new(unsafe { Box::from_raw(sliceptr) }), p: PhantomData })
   }
   
   #[inline(always)]
   pub(crate) fn get(&self) -> &[u8]
   {
      // SAFETY: MaybeUninit is just for dropping it manually, it is never actually "uninitialized" outside of the destructor
      unsafe { self.ptr.assume_init_ref().deref() }
   }
   
   #[inline(always)]
   pub(crate) fn get_mut(&mut self) -> &mut [u8]
   {
      // SAFETY: see the get() method above
      unsafe { self.ptr.assume_init_mut().deref_mut() }
   }
   
   #[inline]
   pub(crate) fn duplicate(&self) -> AllocResult<AlignedBuffer<AlignAs>>
   {
      let mut instance = Self::new(NonZero::new(self.get().len()).ok_or(AllocationError)?)?;
      instance.get_mut().copy_from_slice(self.get());
      Ok(instance)
   }
}

impl<AlignAs> core::ops::Drop for AlignedBuffer<AlignAs>
{
   fn drop(&mut self)
   {
      // SAFETY: The size is checked to be non-zero in the constructor, and core::mem::align_of() always returns valid alignment
      let layout = unsafe { Layout::from_size_align_unchecked(self.get().len(), core::mem::align_of::<AlignAs>() ) };
      // SAFETY: self.ptr is still initialized
      let ptr = unsafe { self.ptr.assume_init_mut().as_mut_ptr() };
      // SAFETY: ptr and layout are the same as in new()
      unsafe { std::alloc::dealloc(ptr, layout) }
      // NOTE: at this point, self.ptr should be considered uninitialized, because it contains a dangling pointer
   }
}

impl<AlignAs> core::clone::Clone for AlignedBuffer<AlignAs>
{
   #[inline]
   fn clone(&self) -> Self
   {
      self.duplicate().unwrap()
   }
}