#![cfg_attr(not(feature = "std"), no_std)]

/// Write trait to use with Encoder
pub trait Write {
    type Error;

    /// Write a single byte.
    fn write(&mut self, byte: u8) -> Result<(), Self::Error>;
}

/// Streaming encoder
///
/// Allows encoding of reverse-COBS messages in a streaming fashion, with almost
/// no memory usage (internal state is just one single byte!).
///
/// To encode a message, call [write](Self::write) for each byte in the message, then call [end](Self::end).
///
/// You may use the same Encoder instance to encode multiple messages. In this case, you
/// will probably want to separate messages with a `0x00`, which you have to write manually
/// after calling [end](Self::end), for example with `encoder.writer().write(0)`.
pub struct Encoder<W> {
    w: W,
    run: u8,
    zeros: u8,
}

// Standard COBS/rCOBS:
//   00000000 => end of frame
//   nnnnnnnn => output n-1 bytes from stream, output 0x00
//   11111111 => output 254 bytes from stream
//
// zCOBS/rzCOBS:
//   00000000 => end of frame
//   0xxxxxxx => foreach x from LSB to MSB: if x=0 output 1 byte from stream, if x=1 output 0x00
//   1nnnnnnn => output n+7 bytes from stream, output 0x00
//   11111111 => output 134 bytes from stream

impl<W> Encoder<W> {
    /// Create a new encoder with the given writer.
    pub const fn new(w: W) -> Self {
        Self {
            w,
            run: 0,
            zeros: 0,
        }
    }

    /// Mutably borrow the inner writer.
    pub fn writer(&mut self) -> &mut W {
        &mut self.w
    }
}

impl<W: Write> Encoder<W> {

    /// Write a message byte.
    pub fn write(&mut self, byte: u8) -> Result<(), W::Error> {
        if self.run < 7 {
            if byte == 0 {
                self.zeros |= 1 << self.run;
            } else {
                self.w.write(byte)?;
            }

            self.run += 1;
            if self.run == 7 && self.zeros != 0x00 {
                self.w.write(self.zeros)?;
                self.run = 0;
                self.zeros = 0;
            }
        } else if byte == 0 {
            self.w.write((self.run - 7) | 0x80)?;
            self.run = 0;
            self.zeros = 0;
        } else {
            self.w.write(byte)?;
            self.run += 1;
            if self.run == 134 {
                self.w.write(0xFF)?;
                self.run = 0;
                self.zeros = 0;
            }
        }
        Ok(())
    }

    /// Finish encoding a message.
    ///
    /// This does NOT write a `0x00` separator byte, you must write it yourself
    /// if you so desire.
    pub fn end(&mut self) -> Result<(), W::Error> {
        match self.run {
            0 => {},
            1..=6 => self.w.write((self.zeros | (0xFF << self.run)) & 0x7F)?,
            _ => self.w.write((self.run - 7) | 0x80)?,
        }
        self.run = 0;
        self.zeros = 0;
        Ok(())
    }
}

/// Encode a full message.
///
/// Encodes a single message and returns it as a `Vec`. The returned data does
/// not include any `0x00` separator byte, you have to add it yourself.
///
/// This is a convenience function using [Encoder] internally. For streaming encoding, use [Encoder].
#[cfg(feature = "std")]
pub fn encode(data: &[u8]) -> Vec<u8> {
    struct VecWriter<'a>(&'a mut Vec<u8>);

    impl<'a> Write for VecWriter<'a> {
        type Error = std::convert::Infallible;
        fn write(&mut self, byte: u8) -> Result<(), Self::Error> {
            self.0.push(byte);
            Ok(())
        }
    }

    let mut res = Vec::new();
    let mut enc = Encoder::new(VecWriter(&mut res));
    for &b in data {
        enc.write(b).unwrap();
    }
    enc.end().unwrap();
    res
}

/// Error indicating the decoded data was malformed reverse-COBS.
#[cfg(feature = "std")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MalformedError;

/// Decode a full message.
///
/// `data` must be a full rzCOBS encoded message. Decoding partial
/// messages is not possible. `data` must NOT include any `0x00` separator byte.
#[cfg(feature = "std")]
pub fn decode(data: &[u8]) -> Result<Vec<u8>, MalformedError> {
    let mut res = vec![];
    let mut data = data.iter().rev().cloned();
    while let Some(x) = data.next() {
        match x {
            0 => return Err(MalformedError),
            0x01..=0x7f => {
                for i in 0..7 {
                    if x & (1 << (6-i)) == 0 {
                        res.push(data.next().ok_or(MalformedError)?);
                    } else {
                        res.push(0);
                    }
                }
            }
            0x80..=0xfe => {
                let n = (x & 0x7f) + 7;
                res.push(0);
                for _ in 0..n {
                    res.push(data.next().ok_or(MalformedError)?);
                }
            }
            0xff => {
                for _ in 0..134 {
                    res.push(data.next().ok_or(MalformedError)?);
                }
            }
        }
    }

    res.reverse();
    Ok(res)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    MalformedError,
    BufferOverflow,
}

/// Decode a full message.
///
/// `data` must be a full rzCOBS encoded message. Decoding partial
/// messages is not possible. `data` must NOT include any `0x00` separator byte.
pub fn decode_to_slice<'a>(data: &[u8], res: &'a mut [u8]) -> Result<&'a mut [u8], DecodeError> {
    struct Vec<'a> {
        data: &'a mut [u8],
        len: usize,
    }

    impl<'a> Vec<'a> {
        fn try_push(&mut self, x: u8) -> Result<(), DecodeError> {
            *self.data.get_mut(self.len).ok_or(DecodeError::BufferOverflow)? = x;
            self.len += 1;

            Ok(())
        }

        fn reverse(&mut self) {
            self.data[..self.len].reverse()
        }
    }

    let mut res = Vec{ data: res, len: 0 };

    let mut data = data.iter().rev().cloned();
    while let Some(x) = data.next() {
        match x {
            0 => return Err(DecodeError::MalformedError),
            0x01..=0x7f => {
                for i in 0..7 {
                    if x & (1 << (6-i)) == 0 {
                        res.try_push(data.next().ok_or(DecodeError::MalformedError)?)?;
                    } else {
                        res.try_push(0)?;
                    }
                }
            }
            0x80..=0xfe => {
                let n = (x & 0x7f) + 7;
                res.try_push(0)?;
                for _ in 0..n {
                    res.try_push(data.next().ok_or(DecodeError::MalformedError)?)?;
                }
            }
            0xff => {
                for _ in 0..134 {
                    res.try_push(data.next().ok_or(DecodeError::MalformedError)?)?;
                }
            }
        }
    }

    res.reverse();
    Ok(&mut res.data[..res.len])
}

#[cfg(feature = "std")]
#[cfg(test)]
mod tests {
    use super::*;
    use hex_literal::hex;

    #[test]
    fn it_works() {
        let tests: &[(&[u8], &[u8])] = &[
            (
                &hex!(""),
                &hex!(""),
            ), (
                &hex!("00"),
                &hex!("7f"),
            ), (
                &hex!("0000"),
                &hex!("7f"),
            ), (
                &hex!("00000000000000"),
                &hex!("7f"),
            ), (
                &hex!("0000000000000000"),
                &hex!("7f7f"),
            ), (
                &hex!("01"),
                &hex!("017e"),
            ), (
                &hex!("0100"),
                &hex!("017e"),
            ), (
                &hex!("0001"),
                &hex!("017d"),
            ), (
                &hex!("0102"),
                &hex!("01027c"),
            ), (
                &hex!("11223344556600"),
                &hex!("11223344556640"),
            ), (
                &hex!("11223344556677"),
                &hex!("1122334455667780"),
            ), (
                &hex!("1122334455667700"),
                &hex!("1122334455667780"),
            ), (
                &hex!("1122334455667788"),
                &hex!("112233445566778881"),
            ), (
                &hex!("00000000000000 000000000000ff"),
                &hex!("7f ff3f"),
            ), (
                &hex!("00000000004400 000000000000ff"),
                &hex!("445f ff3f"),
            ), (
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485"),
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485fe"),
            ), (
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283848500"),
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f808182838485fe"),
            ), (
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283848586"),
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283848586ff"),
            ), (
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f8081828384858600"),
                &hex!("0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f303132333435363738393a3b3c3d3e3f404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f606162636465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f80818283848586ff7f"),
            ),
        ];

        for (dec, enc) in tests {
            assert_eq!(&encode(dec), enc);

            let got = decode(enc).unwrap();
            assert_eq!(&got[..dec.len()], *dec);
            assert!(&got[dec.len()..].iter().all(|&x| x == 0));
        }
    }
}
