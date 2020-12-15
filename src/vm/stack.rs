use std::{
    mem,
    rc::Rc,
    cell::RefCell
};

use crate::common::data::Data;

use crate::vm::{
    tag::Tagged,
    slot::{Slot, Suspend},
};

/// A stack of `Tagged` `Data`.
/// Note that in general the stack is expected to follow the following pattern:
/// ```plain
/// FV...V...F V...T... ...
/// ```
/// Or in other words, a frame followed by a block of *n* values that are locals
/// followed by *n* temporaries, ad infinitum.
#[derive(Debug)]
pub struct Stack {
    pub frames: Vec<usize>,
    pub stack:  Vec<Tagged>
}

impl Stack {
    /// Create a new `Stack` with a single frame.
    pub fn init() -> Stack {
        Stack {
            frames: vec![0],
            stack:  vec![Tagged::frame()],
        }
    }

    #[inline]
    fn frame_index(&self) -> usize {
        *self.frames.last().unwrap()
    }

    #[inline]
    fn pop(&mut self) -> Tagged {
        self.stack.pop()
            .expect("VM tried to pop empty stack, stack should never be empty")
    }

    fn swap(&mut self, index: usize, tagged: Tagged) -> Tagged {
        mem::replace(&mut self.stack[index], tagged)
    }

    /// Pushes some `Data` onto the `Stack`, tagging it along the way
    #[inline]
    pub fn push_data(&mut self, data: Data) {
        self.stack.push(Tagged::new(Slot::Data(data)))
    }

    /// Pushes some `Tagged` `Data` onto the `Stack` without unwrapping it.
    #[inline]
    pub fn push_tagged(&mut self, tagged: Tagged) {
        self.stack.push(tagged)
    }

    /// Pops some `Data` of the `Stack`, panicking if what it pops is not `Data`.
    /// Note that this will never return a `Heaped` value, rather cloning the value inside.
    #[inline]
    pub fn pop_data(&mut self) -> Data {
        let value = self.stack.pop()
            .expect("VM tried to pop empty stack, stack should never be empty");

        match value.slot().data() {
            Data::Heaped(h) => h.borrow().clone(),
            d => d,
        }
    }

    /// Pops a stack frame from the `Stack`, restoring the previous frame.
    /// Panics if there are no frames left on the stack.
    #[inline]
    pub fn pop_frame(&mut self) -> Suspend {
        let index = self.frames.pop().expect("No frame index found");

        if let Slot::Frame = self.pop().slot() {} else {
            unreachable!("Expected frame on top of stack");
        }

        let old_slot = self.swap(index, Tagged::frame()).slot();
        if let Slot::Suspend(s) = old_slot {
            return s;
        } else {
            unreachable!("Expected frame on top of stack");
        }
    }

    /// Pushes a new stack frame onto the `Stack`.
    /// Takes the old suspended closure / ip, and stores that on the stack.
    #[inline]
    pub fn push_frame(&mut self, suspend: Suspend) {
        let frame_index = self.frame_index();
        self.stack[frame_index] = Tagged::new(Slot::Suspend(suspend));
        self.frames.push(self.stack.len());
        self.stack.push(Tagged::frame());
    }

    /// Wraps the top data value on the stack in `Data::Heaped`,
    /// data must not already be on the heap
    #[inline]
    pub fn heapify(&mut self, index: usize) {
        let local_index = self.frame_index() + index + 1;

        let data = self.swap(local_index, Tagged::not_init()).slot().data();
        let heaped = Slot::Data(Data::Heaped(Rc::new(RefCell::new(data))));
        mem::drop(mem::replace(&mut self.stack[local_index], Tagged::new(heaped)));
    }

    pub fn local_slot(&mut self, index: usize) -> Slot {
        let local_index = self.frame_index() + index + 1;

        // a little bit of shuffling involved
        // I know that something better than this can be done
        let slot = self.swap(local_index, Tagged::not_init()).slot();
        let copy = slot.clone();
        mem::drop(self.swap(local_index, Tagged::new(slot)));

        return copy;
    }

    pub fn local_data(&mut self, index: usize) -> Data {
        let local_index = self.frame_index() + index + 1;

        // a little bit of shuffling involved
        // I know that something better than this can be done
        let data = self.swap(local_index, Tagged::not_init()).slot().data();
        let copy = data.clone();
        mem::drop(self.swap(local_index, Tagged::new(Slot::Data(data))));

        return copy;
    }

    /// Sets a local - note that this function doesn't do much.
    /// It's a simple swap-and-drop.
    /// If a new local is being declared,
    /// it's literally a bounds-check and no-op.
    pub fn set_local(&mut self, index: usize) {
        let local_index = self.frame_index() + index + 1;

        if (self.stack.len() - 1) == local_index {
            // local is already in the correct spot; we declare it
            return;
        } else if (self.stack.len() - 1) < local_index {
            println!("{} < {}", self.stack.len() - 1, local_index);
            unreachable!("Can not set local that is not yet on stack");
        } else {
            // get the old local
            let slot = self.swap(local_index, Tagged::not_init()).slot();

            // replace the old value with the new one if on the heap
            let tagged = match slot {
                Slot::Frame => unreachable!("Expected data, found frame"),
                // if it is on the heap, we replace in the old value
                Slot::Data(Data::Heaped(ref cell)) => {
                    // TODO: check types?
                    mem::drop(cell.replace(self.pop_data()));
                    Tagged::new(slot)
                }
                // if it's not on the heap, we assume it's data,
                // and do a quick swap-and-drop
                _ => self.stack.pop().unwrap(),
            };

            mem::drop(self.swap(local_index, tagged))
        }
    }
}
