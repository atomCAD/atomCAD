// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{mem, slice};

use winit::event::{DeviceEvent, WindowEvent};

mod bounding_box;
pub mod ids;

pub use bounding_box::BoundingBox;

pub enum InputEvent<'a> {
    Window(WindowEvent<'a>),
    Device(DeviceEvent),
    BeginningFrame,
}

/// # Safety
///
/// This is safe because it merely exposes the backing memory use natively to
/// store the instance of the type as a byte array.  It is tagged unsafe only
/// because of the pointer and slice operations involved.
///
/// Still, even though this won't result in memory leaks or dereferencing NULL,
/// it is still moderately unsafe as the direct memory storage layout may
/// change across architectures.  Be very careful with what you store or you
/// will get inconsistent results across platforms.
pub unsafe trait AsBytes {
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self as *const _ as *const u8, mem::size_of_val(self)) }
    }
}

macro_rules! impl_as_bytes {
    ($ty:ty) => {
        unsafe impl AsBytes for $ty {}
    };
    ($($ty:ty),*) => {
        $(
            impl_as_bytes!($ty);
        )*
    };
}

impl_as_bytes!(
    (),
    ultraviolet::Vec2,
    ultraviolet::Vec3,
    ultraviolet::Mat2,
    ultraviolet::Mat3,
    ultraviolet::Mat4,
    ultraviolet::Rotor2,
    ultraviolet::Rotor3
);

unsafe impl<T> AsBytes for [T]
where
    T: AsBytes + Sized,
{
    fn as_bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.as_ptr().cast(), mem::size_of_val(self)) }
    }
}

// Returns true if the `target` point is inside the specified cylinder. The cylinder is
// described by the point at the center of each endcap circle as well as the radius of the
// endcap circle. `radius_sq = radius * radius`.
pub fn inside_cylinder(
    p1: ultraviolet::Vec3,
    p2: ultraviolet::Vec3,
    radius_sq: f32,
    target: ultraviolet::Vec3,
) -> bool {
    // If p1 == p2, the cylinder's orientation is not well defined. We only consider a point
    // to be inside this null cylinder if it is exactly coincident with those points:
    if p1 == p2 {
        return p1 == target;
    }

    // Now, we consider cylinders that have nonzero volume.
    // First,  We translate the space so that p1 is at the origin (i.e. we subtract p1
    // from all points) - this leaves us in a simpler case where the cylinder's centerline is
    // just described by the vector p2 - p1, with no offset to consider.
    let cyl_axis = p2 - p1;
    let target = target - p1;

    // Then, imagine projecting the target point vector orthogonally onto the centerline.
    // This forms a new vector colinear with p2 - p1: so we can write it as alpha * (p2 - p1).
    let alpha = cyl_axis.dot(target) / cyl_axis.mag_sq();
    let proj = alpha * cyl_axis;

    // If alpha is zero, the target point is in the plane of the p1 endcap. If it is 1, the
    // target point is in the plane of the p2 endcap. If it is in between 0 and 1, it is in
    // between those two planes. If it is < 0 or > 1, it is absolutely outside of the cylinder.
    // To be clear, this just tells us if the target point is inside the 'infinite-radius
    // cylinder' - if one existed.
    if !(0.0..=1.0).contains(&alpha) {
        return false;
    }

    // Finally, we just consider the radius - how long is the line from alpha * (p2 - p1) to
    // `target`?
    let orthogonal_distance_sq = (target - proj).mag_sq();
    orthogonal_distance_sq <= radius_sq
}

// End of File
