use std::{
    mem,
    f64,
    fmt::{Formatter, Debug, Error},
};

// TODO: implement stack Frame

use crate::common::data::Data;

/// Tagged implements Nan-tagging around the `Data` enum.
/// In essence, it's possible to exploit the representation of f64 NaNs
/// to store pointers to other datatypes.
/// When layed out, this is what the bit-level representation of a
/// double-precision floating-point number looks like.
/// Below is the bit-level layout of a tagged NaN.
/// ```plain
/// SExponent---QIMantissa------------------------------------------
/// PNaN--------11D-Payload---------------------------------------TT
/// ```
/// Where `S` is sign, `Q` is quiet flag, `I` is Intel’s “QNan Floating-Point Indefinite”,
/// `P` is pointer flag, `D` is Data Tag (should always be 1), `T` is Tag.
/// We have 2 tag bits, 4 values: 00 is unit '()', 10 is false, 11 is true,
/// but this might change if I figure out what to do with them
/// NOTE: maybe add tag bit for 'unit'
/// NOTE: implementation modeled after:
/// https://github.com/rpjohnst/dejavu/blob/master/gml/src/vm/value.rs
/// and the Optimization chapter from Crafting Interpreters
/// Thank you!
pub struct Tagged(u64);

const QNAN:   u64 = 0x7ffe_0000_0000_0000;
const P_FLAG: u64 = 0x8000_0000_0000_0000;
const P_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
const U_FLAG: u64 = 0x0000_0000_0000_0000;
const F_FLAG: u64 = 0x0000_0000_0000_0010;
const T_FLAG: u64 = 0x0000_0000_0000_0011;

impl Tagged {
    /// Wraps `Data` to create a new tagged pointer.
    pub fn new(data: Data) -> Tagged {
        match data {
            // Real
            Data::Real(f) => Tagged(f.to_bits()),
            // Unit
            Data::Unit => Tagged(QNAN | U_FLAG),
            // True and false
            Data::Boolean(false) => Tagged(QNAN | F_FLAG),
            Data::Boolean(true)  => Tagged(QNAN | T_FLAG),

            // on the heap
            // TODO: layout to make sure pointer is the right size when boxing
            other => Tagged(P_FLAG | QNAN | (P_MASK & (Box::into_raw(Box::new(other))) as u64)),
        }
    }

    // TODO: encode frame in tag itself; a frame is not data
    pub fn frame() -> Tagged {
        Tagged::new(Data::Frame)
    }

    /// Returns the underlying data or a pointer to that data.
    pub fn extract(&self) -> Result<Data, Box<Data>> {
        println!("-- Extracting...");
        let Tagged(bits) = self;
        mem::forget(self);

        return match bits {
            n if (n & QNAN) != QNAN    => Ok(Data::Real(f64::from_bits(*n))),
            u if u == &(QNAN | U_FLAG) => Ok(Data::Unit),
            f if f == &(QNAN | F_FLAG) => Ok(Data::Boolean(false)),
            t if t == &(QNAN | T_FLAG) => Ok(Data::Boolean(true)),
            p if (p & P_FLAG) == P_FLAG => Err(
                unsafe {
                    Box::from_raw((bits & P_MASK) as *mut Data)
                }
            ),
            _ => unreachable!("Corrupted tagged data"),
        }
    }

    // TODO: use deref trait
    // Can't for not because of E0515 caused by &Data result
    /// Unwrapps a tagged number into the appropriate datatype.
    pub fn data(self) -> Data {
        match self.extract() {
            Ok(data) => data,
            Err(boxed) => *boxed,
        }
    }

    pub fn copy(&self) -> Data {
        match self.extract() {
            Ok(data) => data.to_owned(),
            Err(boxed) => {
                let copy = *boxed.clone();
                mem::forget(boxed);
                copy
            },
        }
    }
}

// TODO: verify this works as intended
impl Drop for Tagged {
    fn drop(&mut self) {
        match self.extract() {
            Ok(_data) => (),
            Err(boxed) => mem::drop(*boxed),
        }

        mem::drop(self);
    }
}

impl Debug for Tagged {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        // let Tagged(bits) = &self;
        // let pointer = P_FLAG | QNAN;
        //
        // if (pointer & bits) == pointer {
        //     let item = unsafe { Box::from_raw((bits & P_MASK) as *mut Data) };
        //     write!(f, "Data {:?}", item)?;
        //     mem::forget(item);
        // } else {
        //     write!(f, "Real {}", f64::from_bits(bits.clone()))?;
        // }
        //
        // Ok(())
        // // write an exact copy of the data
        write!(f, "Tagged({:?})", self.copy())
    }
}

impl From<Tagged> for u64 {
    /// Unwraps a tagged pointer into the literal representation for debugging.
    fn from(tagged: Tagged) -> Self { tagged.0 }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn reals_eq() {
        let positive = 478_329.0;
        let negative = -231.0;
        let nan      = f64::NAN;
        let neg_inf  = f64::NEG_INFINITY;

        for n in &[positive, negative, nan, neg_inf] {
            let data    = Data::Real(*n);
            let wrapped = Tagged::new(data);
            match wrapped.data() {
                Data::Real(f) if f.is_nan() => assert!(n.is_nan()),
                Data::Real(f) => assert_eq!(*n, f),
                _             => panic!("Didn't unwrap to a real"),
            }
        }
    }

    #[test]
    fn bool_and_back() {
        assert_eq!(Data::Boolean(true),  Tagged::new(Data::Boolean(true) ).data());
        assert_eq!(Data::Boolean(false), Tagged::new(Data::Boolean(false)).data());
    }

    #[test]
    fn unit() {
        assert_eq!(Data::Unit, Tagged::new(Data::Unit).data());
    }

    #[test]
    fn size() {
        let data_size = mem::size_of::<Data>();
        let tag_size  = mem::size_of::<Tagged>();

        // Tag == u64 == f64 == 64
        // If the tag is larger than the data, we're doing something wrong
        assert_eq!(tag_size, mem::size_of::<f64>());
        assert!(tag_size < data_size);
    }

    #[test]
    fn string_pointer() {
        let s =     "I just lost the game".to_string();
        let three = "Elongated Muskrat".to_string();
        let x =     "It's kind of a dead giveaway, isn't it?".to_string();

        for item in &[s, three, x] {
            let data    = Data::String(item.clone());
            let wrapped = Tagged::new(data);
            // println!("{:#b}", u64::from(wrapped));
            match wrapped.data() {
                Data::String(s) => { assert_eq!(item, &s) },
                other           => {
                    // println!("{:#b}", u64::from(wrapped));
                    panic!("Didn't unwrap to a string");
                },
            }
        }
    }

    #[test]
    fn other_tests_eq() {
        let tests = vec![
            Data::Real(f64::consts::PI),
            Data::Real(-2.12),
            Data::Real(2.5E10),
            Data::Real(2.5e10),
            Data::Real(2.5E-10),
            Data::Real(0.5),
            Data::Real(f64::MAX),
            Data::Real(f64::MIN),
            Data::Real(f64::INFINITY),
            Data::Real(f64::NEG_INFINITY),
            Data::Real(f64::NAN),
            Data::String("Hello, World!".to_string()),
            Data::String("".to_string()),
            Data::String("Whoop 😋".to_string()),
            Data::Boolean(true),
            Data::Boolean(false),
            Data::Unit,
        ];

        for test in tests {
            println!("{:?}", test);
            let tagged = Tagged::new(test.clone());
            println!("{:?}", tagged);
            println!("{:#b}", tagged.0);
            let untagged = tagged.data();
            println!("{:?}", untagged);
            println!("---");

            if let Data::Real(f) = untagged {
                if let Data::Real(n) = test {
                    if n.is_nan() {
                        assert!(f.is_nan())
                    } else {
                        assert_eq!(test, Data::Real(n));
                    }
                }
            } else {
                assert_eq!(test, untagged);
            }
        }
    }
}
