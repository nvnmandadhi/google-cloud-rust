// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Hand-written encoding of rows in the [protobuf wire format].
//!
//! A protobuf message is a sequence of fields. Each field is a *tag* (the
//! field number and a wire type), followed by the value. Field names never
//! appear on the wire; the schema maps field numbers to column names.
//!
//! [protobuf wire format]: https://protobuf.dev/programming-guides/encoding/

/// The wire type for length-delimited values, such as strings.
const LENGTH_DELIMITED: u32 = 2;

/// The number of low bits in a tag that hold the wire type.
const WIRE_TYPE_BITS: u32 = 3;

/// The number of payload bits in each varint byte.
const VARINT_PAYLOAD_BITS: u32 = 7;

/// Selects the payload bits of a varint byte.
const VARINT_PAYLOAD_MASK: u64 = 0x7F;

/// Marks every varint byte except the last one.
const VARINT_CONTINUATION: u8 = 0x80;

/// Appends `value` as a [varint]: 7 bits per byte, least significant first.
///
/// [varint]: https://protobuf.dev/programming-guides/encoding/#varints
fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value > VARINT_PAYLOAD_MASK {
        buf.push(((value & VARINT_PAYLOAD_MASK) as u8) | VARINT_CONTINUATION);
        value >>= VARINT_PAYLOAD_BITS;
    }
    buf.push(value as u8);
}

/// Appends a tag, which packs the field number and the wire type in a varint.
fn encode_tag(field_number: u32, wire_type: u32, buf: &mut Vec<u8>) {
    encode_varint(u64::from((field_number << WIRE_TYPE_BITS) | wire_type), buf);
}

/// Appends a `string` field: the tag, the length in bytes, and the UTF-8 bytes.
fn encode_string(field_number: u32, value: &str, buf: &mut Vec<u8>) {
    encode_tag(field_number, LENGTH_DELIMITED, buf);
    encode_varint(value.len() as u64, buf);
    buf.extend_from_slice(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    /// The field number of the `name` column in our one-column row.
    const NAME_FIELD: u32 = 1;

    /// A sample value for the `name` column.
    const NAME: &str = "alice";

    /// What `prost` generates for `message Row { string name = 1; }`.
    #[derive(Clone, PartialEq, Message)]
    struct ProstRow {
        #[prost(string, tag = "1")]
        name: String,
    }

    fn encode_row(name: &str) -> Vec<u8> {
        let mut buf = Vec::new();
        encode_string(NAME_FIELD, name, &mut buf);
        buf
    }

    #[test]
    fn encode_by_hand() {
        let mut want = vec![
            0x0A, // tag: (field number 1 << 3) | wire type 2 (length-delimited)
            0x05, // length: "alice" is 5 bytes
        ];
        want.extend_from_slice(NAME.as_bytes());
        assert_eq!(encode_row(NAME), want);
    }

    #[test]
    fn matches_prost() {
        let want = ProstRow {
            name: NAME.to_string(),
        }
        .encode_to_vec();
        assert_eq!(encode_row(NAME), want);
    }

    #[test]
    fn long_string_uses_multi_byte_length() {
        const LEN: usize = 300;
        let name = "a".repeat(LEN);
        let got = encode_row(&name);
        // 300 needs 9 bits, but a varint byte holds 7, so it takes two bytes:
        //   300 = 0b10_0101100
        //   0101100 + continuation bit 1 -> 0b1010_1100 = 0xAC
        //   0000010 + continuation bit 0 -> 0b0000_0010 = 0x02
        assert_eq!(got[..3], [0x0A, 0xAC, 0x02]);
        let want = ProstRow { name }.encode_to_vec();
        assert_eq!(got, want);
    }

    #[test]
    fn empty_string_is_encoded() {
        // We always write the field, even if the value is empty.
        assert_eq!(encode_row(""), [0x0A, 0x00]);
        // `prost` skips default values. BigQuery reads a missing field as
        // NULL, so relying on `prost` here would turn "" into NULL.
        let encoded = ProstRow {
            name: String::new(),
        }
        .encode_to_vec();
        assert!(encoded.is_empty(), "{encoded:?}");
    }
}
