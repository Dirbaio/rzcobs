# rzcobs

[![Documentation](https://docs.rs/rzcobs/badge.svg)](https://docs.rs/rzcobs)

Reverse-Zerocompressing-COBS encoding (rzCOBS) is a variant of [rCOBS](https://github.com/Dirbaio/rcobs) which aditionally compresses
messages known to contain many zero bytes.

Standard COBS/rCOBS encoding:

    00000000 => end of frame
    nnnnnnnn => output n-1 bytes from stream, output 0x00
    11111111 => output 254 bytes from stream

rzCOBS encoding:

    00000000 => end of frame
    0xxxxxxx => foreach x from LSB to MSB: if x=0 output 1 byte from stream, if x=1 output 0x00
    1nnnnnnn => output n+7 bytes from stream, output 0x00
    11111111 => output 134 bytes from stream

Zero-compression is achieved by splitting the input in chunks of 7 bytes and encoding in a bitfield
which are zeros, and then transmitting only the non-zero bytes. If there are hore than 7 non-zero bytes in 
a row, encoding works COBS-like, where the number of non-zero bytes is emitted. The maximum overhead is therefore
ceil(n/134) bytes, for a message of n bytes.

When a message is encoded and then decoded, the result is the original message, with up to 6 zero bytes appended.
Higher layer protocols must be able to deal with these appended zero bytes.

## License

This work is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
