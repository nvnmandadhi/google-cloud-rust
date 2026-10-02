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

use super::wire_format::{
    bool_field, bytes_field, double_field, encode_bool, encode_bytes, encode_double, encode_float,
    encode_int64, encode_string, float_field, int64_field, string_field,
};
use crate::error::ConvertError;
use crate::model::ProtoSchema;
use bytes::Bytes;
use wkt::{DescriptorProto, FieldDescriptorProto};

/// A trait for converting Rust types into rows for the [Proto] data format.
///
/// A writer that uses the [Proto] data format needs two things: a schema,
/// which describes the rows, and the rows themselves, encoded as protocol
/// buffers. Types that implement this trait provide both.
///
/// Use [`#[derive(ToRow)]`](derive@crate::write::ToRow) to implement this
/// trait. Each field is written to the column with the same name. Use
/// `#[bigquery(rename = "column_name")]` when the names differ.
///
/// # Supported types
///
/// | Rust type | BigQuery type |
/// | --- | --- |
/// | `bool` | `BOOL` |
/// | `i32`, `i64` | `INT64` |
/// | `f32`, `f64` | `FLOAT64` |
/// | `String` | `STRING` |
/// | `Vec<u8>`, [`Bytes`](bytes::Bytes) | `BYTES` |
/// | [`Timestamp`](wkt::Timestamp) | `TIMESTAMP` |
/// | `Option<T>` | The type for `T`. `None` writes `NULL`. |
///
/// BigQuery keeps microseconds, so the nanoseconds in a
/// [`Timestamp`](wkt::Timestamp) are rounded down.
///
/// # Example
///
/// ```
/// # use google_cloud_bigquery::client::Write;
/// # use google_cloud_bigquery::model::ProtoRows;
/// use google_cloud_bigquery::write::ToRow;
///
/// #[derive(ToRow)]
/// struct Row {
///     name: String,
///     age: Option<i64>,
/// }
///
/// # async fn sample(client: Write) -> anyhow::Result<()> {
/// let writer = client
///     .open_default_stream("projects/my-project/datasets/my_dataset/tables/my_table")
///     .build_proto(Row::schema())
///     .await?;
/// let rows = [
///     Row { name: "alice".to_string(), age: Some(30) },
///     // `None` writes `NULL` to the `age` column.
///     Row { name: "bob".to_string(), age: None },
/// ];
/// let serialized_rows = rows.iter().map(Row::to_row).collect::<Result<Vec<_>, _>>()?;
/// writer
///     .append(ProtoRows::new().set_serialized_rows(serialized_rows))
///     .send()
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// [Proto]: crate::write::format::Proto
pub trait ToRow {
    /// Returns the schema for rows of this type.
    ///
    /// Use it to create a writer with [build_proto].
    ///
    /// [build_proto]: crate::builder::write::WriterBuilder::build_proto
    fn schema() -> ProtoSchema;

    /// Encodes `self` as one row, in the protobuf wire format.
    fn to_row(&self) -> Result<Bytes, ConvertError>;
}

/// A Rust type that can be a field in a row.
///
/// This is an implementation detail of [ToRow], it is not part of the public
/// API.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a supported field type for `#[derive(ToRow)]`",
    label = "unsupported field type",
    note = "see the `ToRow` documentation for the supported types"
)]
pub trait ProtoValue {
    /// Describes a field of this type, with the given name and field number.
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto;

    /// Appends `self` to `buf`, as the field with the given number.
    ///
    /// BigQuery reads a missing field as `NULL`, so implementations append the
    /// field even for default values such as `""` or `0`. Only `None` leaves
    /// the field out.
    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError>;
}

impl ProtoValue for String {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        string_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_string(number, self, buf);
        Ok(())
    }
}

impl ProtoValue for i64 {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        int64_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_int64(number, *self, buf);
        Ok(())
    }
}

impl ProtoValue for i32 {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        // `INT64` columns also take `int32` fields, but the bytes would be the
        // same: protobuf sign-extends negative `int32` values to 64 bits.
        int64_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_int64(number, i64::from(*self), buf);
        Ok(())
    }
}

impl ProtoValue for bool {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        bool_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_bool(number, *self, buf);
        Ok(())
    }
}

impl ProtoValue for f64 {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        double_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_double(number, *self, buf);
        Ok(())
    }
}

impl ProtoValue for f32 {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        // BigQuery converts `float` fields to `FLOAT64` values. They take 4
        // bytes instead of 8, which keeps rows smaller.
        float_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_float(number, *self, buf);
        Ok(())
    }
}

impl ProtoValue for Vec<u8> {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        bytes_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_bytes(number, self, buf);
        Ok(())
    }
}

impl ProtoValue for Bytes {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        bytes_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_bytes(number, self, buf);
        Ok(())
    }
}

impl ProtoValue for wkt::Timestamp {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        // BigQuery takes `TIMESTAMP` values as microseconds since the Unix
        // epoch, in an `int64` field.
        int64_field(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        encode_int64(number, timestamp_micros(self), buf);
        Ok(())
    }
}

/// The number of microseconds in a second.
const MICROS_PER_SECOND: i64 = 1_000_000;

/// The number of nanoseconds in a microsecond.
const NANOS_PER_MICRO: i32 = 1_000;

/// Returns the microseconds since the Unix epoch, rounded down.
///
/// The nanoseconds in a [wkt::Timestamp] are never negative, even before the
/// epoch, so dividing them rounds down. The result cannot overflow: a
/// [wkt::Timestamp] is between the years 1 and 9999, like a BigQuery
/// `TIMESTAMP`.
fn timestamp_micros(timestamp: &wkt::Timestamp) -> i64 {
    timestamp.seconds() * MICROS_PER_SECOND + i64::from(timestamp.nanos() / NANOS_PER_MICRO)
}

impl<T: ProtoValue> ProtoValue for Option<T> {
    fn field_descriptor(name: &str, number: u32) -> FieldDescriptorProto {
        // All fields are optional in the schema, so `NULL` needs no changes.
        T::field_descriptor(name, number)
    }

    fn encode(&self, number: u32, buf: &mut Vec<u8>) -> Result<(), ConvertError> {
        match self {
            Some(value) => value.encode(number, buf),
            // BigQuery reads a missing field as `NULL`.
            None => Ok(()),
        }
    }
}

/// Returns the schema for a message with the given name and fields.
///
/// This is an implementation detail of [ToRow], it is not part of the public
/// API.
pub fn message_schema<I>(name: &str, fields: I) -> ProtoSchema
where
    I: IntoIterator<Item = FieldDescriptorProto>,
{
    let descriptor = DescriptorProto::new().set_name(name).set_field(fields);
    ProtoSchema::new().set_proto_descriptor(descriptor)
}

#[cfg(test)]
mod tests {
    use super::{ProtoValue, timestamp_micros};
    use crate::google::cloud::bigquery::storage::v1;
    use crate::write::ToRow;
    use bytes::Bytes;
    use gaxi::prost::ToProto;
    use prost::Message;
    use test_case::test_case;
    use wkt::Timestamp;

    // Generated code runs in the application's crate, so it names everything
    // through `google_cloud_bigquery::...` paths. This makes those paths work
    // inside this crate too.
    use crate as google_cloud_bigquery;

    /// The table we write to. The mock server accepts any name.
    const TABLE: &str = "projects/p/datasets/d/tables/t";

    /// The name of the message that describes our row.
    const ROW: &str = "Row";

    /// The name of the first column in our row.
    const NAME_COLUMN: &str = "name";

    /// The field number of the `name` column.
    const NAME_FIELD: u32 = 1;

    /// A sample value for the `name` column.
    const NAME: &str = "alice";

    /// The name of the second column in our row.
    const COUNT_COLUMN: &str = "count";

    /// The field number of the `count` column.
    const COUNT_FIELD: u32 = 2;

    /// A sample value for the `count` column. It is not zero, because `prost`
    /// skips zeros, and then there would be nothing to compare.
    const COUNT: i64 = 42;

    /// The name of the third column in our row.
    const CREATED_AT_COLUMN: &str = "created_at";

    /// The field number of the `created_at` column.
    const CREATED_AT_FIELD: u32 = 3;

    /// A sample value for the `created_at` column, with nanoseconds.
    const CREATED_AT: &str = "2025-05-16T09:46:12.123456789Z";

    /// `CREATED_AT` in microseconds since the Unix epoch. The last three
    /// digits of the nanoseconds are dropped.
    const CREATED_AT_MICROS: i64 = 1_747_388_772_123_456;

    /// The name of the fourth column in our row, which can be `NULL`.
    const NICKNAME_COLUMN: &str = "nickname";

    /// The field number of the `nickname` column.
    const NICKNAME_FIELD: u32 = 4;

    /// A sample value for the `nickname` column.
    const NICKNAME: &str = "ally";

    /// The earliest BigQuery `TIMESTAMP`, 0001-01-01 00:00:00 UTC, in
    /// microseconds since the Unix epoch.
    const MIN_MICROS: i64 = -62_135_596_800_000_000;

    /// The latest BigQuery `TIMESTAMP`, 9999-12-31 23:59:59.999999 UTC, in
    /// microseconds since the Unix epoch.
    const MAX_MICROS: i64 = 253_402_300_799_999_999;

    /// A row with `STRING`, `INT64`, `TIMESTAMP`, and nullable `STRING`
    /// columns.
    #[derive(ToRow)]
    struct Row {
        name: String,
        count: i64,
        created_at: Timestamp,
        nickname: Option<String>,
    }

    /// What `prost` generates for:
    ///
    /// `message Row { string name = 1; int64 count = 2; int64 created_at = 3;
    /// optional string nickname = 4; }`
    #[derive(Clone, PartialEq, Message)]
    struct ProstRow {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(int64, tag = "2")]
        count: i64,
        /// BigQuery takes `TIMESTAMP` values as microseconds since the epoch.
        #[prost(int64, tag = "3")]
        created_at: i64,
        /// Like `ToRow`, `prost` writes any `Some` value, even an empty
        /// string, and leaves out `None`.
        #[prost(string, optional, tag = "4")]
        nickname: Option<String>,
    }

    /// A row with the sample values.
    fn sample_row() -> anyhow::Result<Row> {
        Ok(Row {
            name: NAME.to_string(),
            count: COUNT,
            created_at: Timestamp::try_from(CREATED_AT)?,
            nickname: Some(NICKNAME.to_string()),
        })
    }

    /// The `prost` message with the same values as `sample_row()`.
    fn sample_prost_row() -> ProstRow {
        ProstRow {
            name: NAME.to_string(),
            count: COUNT,
            created_at: CREATED_AT_MICROS,
            nickname: Some(NICKNAME.to_string()),
        }
    }

    /// What BigQuery receives for `Row::schema()`, written by hand.
    fn sent_descriptor() -> anyhow::Result<prost_types::DescriptorProto> {
        use prost_types::field_descriptor_proto::Type;
        Ok(prost_types::DescriptorProto {
            name: Some(ROW.to_string()),
            field: vec![
                sent_field(NAME_COLUMN, NAME_FIELD, Type::String)?,
                sent_field(COUNT_COLUMN, COUNT_FIELD, Type::Int64)?,
                sent_field(CREATED_AT_COLUMN, CREATED_AT_FIELD, Type::Int64)?,
                // `Option<String>` has the same schema as `String`.
                sent_field(NICKNAME_COLUMN, NICKNAME_FIELD, Type::String)?,
            ],
            ..Default::default()
        })
    }

    /// What BigQuery receives for one optional field, written by hand.
    fn sent_field(
        name: &str,
        number: u32,
        field_type: prost_types::field_descriptor_proto::Type,
    ) -> anyhow::Result<prost_types::FieldDescriptorProto> {
        use prost_types::field_descriptor_proto::Label;
        Ok(prost_types::FieldDescriptorProto {
            name: Some(name.to_string()),
            number: Some(i32::try_from(number)?),
            label: Some(Label::Optional as i32),
            r#type: Some(field_type as i32),
            ..Default::default()
        })
    }

    #[test]
    fn schema_matches_descriptor() -> anyhow::Result<()> {
        // Convert the schema the same way the client does before sending it.
        let got: v1::ProtoSchema = Row::schema().to_proto()?;
        assert_eq!(got.proto_descriptor, Some(sent_descriptor()?));
        Ok(())
    }

    #[test]
    fn to_row_matches_prost() -> anyhow::Result<()> {
        assert_eq!(sample_row()?.to_row()?, sample_prost_row().encode_to_vec());
        Ok(())
    }

    #[test]
    fn default_values_are_encoded() -> anyhow::Result<()> {
        let row = Row {
            name: String::new(),
            count: 0,
            created_at: Timestamp::default(),
            nickname: Some(String::new()),
        };
        let want = [
            0x0A, 0x00, // name: tag (field 1, length-delimited), length 0
            0x10, 0x00, // count: tag (field 2, varint), value 0
            0x18, 0x00, // created_at: tag (field 3, varint), the epoch is 0
            0x22, 0x00, // nickname: tag (field 4, length-delimited), length 0
        ];
        assert_eq!(row.to_row()?, want.as_slice());
        // `prost` skips default values, which BigQuery would read as NULLs.
        let encoded = ProstRow::default().encode_to_vec();
        assert!(encoded.is_empty(), "{encoded:?}");
        Ok(())
    }

    #[test]
    fn none_is_left_out() -> anyhow::Result<()> {
        let row = Row {
            nickname: None,
            ..sample_row()?
        };
        // There is no `nickname` field, so BigQuery writes `NULL`.
        let want = ProstRow {
            nickname: None,
            ..sample_prost_row()
        };
        assert_eq!(row.to_row()?, want.encode_to_vec());
        Ok(())
    }

    #[test]
    fn some_zero_is_not_none() -> anyhow::Result<()> {
        let mut buf = Vec::new();
        None::<i64>.encode(COUNT_FIELD, &mut buf)?;
        assert!(buf.is_empty(), "{buf:?}");
        Some(0_i64).encode(COUNT_FIELD, &mut buf)?;
        assert_eq!(buf, [0x10, 0x00]); // tag (field 2, varint), value 0
        Ok(())
    }

    /// Returns the protobuf type of a field of type `T`. The name and number
    /// do not matter.
    fn field_type<T: ProtoValue>() -> wkt::field_descriptor_proto::Type {
        T::field_descriptor(NAME_COLUMN, NAME_FIELD).r#type
    }

    #[test]
    fn field_types() {
        use wkt::field_descriptor_proto::Type;
        assert_eq!(field_type::<bool>(), Type::Bool);
        assert_eq!(field_type::<i32>(), Type::Int64);
        assert_eq!(field_type::<i64>(), Type::Int64);
        assert_eq!(field_type::<f32>(), Type::Float);
        assert_eq!(field_type::<f64>(), Type::Double);
        assert_eq!(field_type::<String>(), Type::String);
        assert_eq!(field_type::<Vec<u8>>(), Type::Bytes);
        assert_eq!(field_type::<Bytes>(), Type::Bytes);
        assert_eq!(field_type::<Timestamp>(), Type::Int64);
        assert_eq!(field_type::<Option<String>>(), Type::String);
    }

    /// A sample value for the `level` column. It is negative, because
    /// protobuf sign-extends negative values to 64 bits.
    const LEVEL: i32 = -7;

    /// A sample value for the `ratio` column.
    const RATIO: f64 = 0.25;

    /// A sample value for the `score` column.
    const SCORE: f32 = 1.5;

    /// A sample value for the `payload` and `digest` columns. It is not valid
    /// UTF-8, which `BYTES` columns allow.
    const PAYLOAD: &[u8] = &[0x00, 0xFF];

    /// A row with the scalar types that `Row` does not have.
    #[derive(Default, ToRow)]
    struct Scalars {
        active: bool,
        level: i32,
        ratio: f64,
        score: f32,
        payload: Vec<u8>,
        digest: Bytes,
    }

    /// What `prost` generates for:
    ///
    /// `message Scalars { bool active = 1; int64 level = 2; double ratio = 3;
    /// float score = 4; bytes payload = 5; bytes digest = 6; }`
    #[derive(Clone, PartialEq, Message)]
    struct ProstScalars {
        #[prost(bool, tag = "1")]
        active: bool,
        /// `i32` values are sent as `int64`.
        #[prost(int64, tag = "2")]
        level: i64,
        #[prost(double, tag = "3")]
        ratio: f64,
        #[prost(float, tag = "4")]
        score: f32,
        #[prost(bytes = "vec", tag = "5")]
        payload: Vec<u8>,
        #[prost(bytes = "bytes", tag = "6")]
        digest: Bytes,
    }

    #[test]
    fn scalars_match_prost() -> anyhow::Result<()> {
        let row = Scalars {
            active: true,
            level: LEVEL,
            ratio: RATIO,
            score: SCORE,
            payload: PAYLOAD.to_vec(),
            digest: Bytes::from_static(PAYLOAD),
        };
        let want = ProstScalars {
            active: true,
            level: i64::from(LEVEL),
            ratio: RATIO,
            score: SCORE,
            payload: PAYLOAD.to_vec(),
            digest: Bytes::from_static(PAYLOAD),
        };
        assert_eq!(row.to_row()?, want.encode_to_vec());
        Ok(())
    }

    #[test]
    fn default_scalars_are_encoded() -> anyhow::Result<()> {
        let mut want: Vec<u8> = Vec::new();
        want.extend([0x08, 0x00]); // active: tag (field 1, varint), false
        want.extend([0x10, 0x00]); // level: tag (field 2, varint), 0
        want.push(0x19); // ratio: tag (field 3, 64-bit), then 0.0 in 8 bytes
        want.extend([0x00; 8]);
        want.push(0x25); // score: tag (field 4, 32-bit), then 0.0 in 4 bytes
        want.extend([0x00; 4]);
        want.extend([0x2A, 0x00]); // payload: tag (field 5, length-delimited), length 0
        want.extend([0x32, 0x00]); // digest: tag (field 6, length-delimited), length 0
        assert_eq!(Scalars::default().to_row()?, want);
        // `prost` skips default values, which BigQuery would read as NULLs.
        let encoded = ProstScalars::default().encode_to_vec();
        assert!(encoded.is_empty(), "{encoded:?}");
        Ok(())
    }

    #[test_case(0, 0, 0; "epoch")]
    #[test_case(1, 1_999, 1_000_001; "drops the last nanosecond digits")]
    #[test_case(-1, 999_999_500, -1; "rounds down before the epoch")]
    fn timestamp_micros_rounds_down(seconds: i64, nanos: i32, want: i64) -> anyhow::Result<()> {
        let timestamp = Timestamp::new(seconds, nanos)?;
        assert_eq!(timestamp_micros(&timestamp), want);
        Ok(())
    }

    #[test]
    fn timestamp_micros_does_not_overflow() -> anyhow::Result<()> {
        // The earliest and latest `wkt::Timestamp` values are also the earliest
        // and latest BigQuery `TIMESTAMP` values.
        let min = Timestamp::new(Timestamp::MIN_SECONDS, Timestamp::MIN_NANOS)?;
        assert_eq!(timestamp_micros(&min), MIN_MICROS);
        let max = Timestamp::new(Timestamp::MAX_SECONDS, Timestamp::MAX_NANOS)?;
        assert_eq!(timestamp_micros(&max), MAX_MICROS);
        Ok(())
    }

    #[tokio::test]
    async fn append_to_mock_server() -> anyhow::Result<()> {
        use crate::client::Write;
        use crate::model::ProtoRows;
        use bigquery_grpc_mock::google::cloud::bigquery::storage::v1 as mock_v1;
        use bigquery_grpc_mock::{MockBigQueryWrite, start};
        use gaxi::grpc::tonic::Response as TonicResponse;
        use google_cloud_auth::credentials::anonymous::Builder as Anonymous;
        use mock_v1::append_rows_request::Rows;
        use mock_v1::append_rows_response::{AppendResult, Response};
        use tokio::sync::{mpsc, oneshot};

        // The mock server hands us the requests it receives, and replies with
        // the responses we queue.
        let (requests_tx, requests_rx) = oneshot::channel();
        let (response_tx, response_rx) = mpsc::channel(1);
        let mut mock = MockBigQueryWrite::new();
        mock.expect_append_rows().return_once(move |request| {
            let _ = requests_tx.send(request.into_inner());
            Ok(TonicResponse::from(response_rx))
        });
        let (endpoint, _server) = start("0.0.0.0:0", mock).await?;

        // This is what an application writes with `ToRow`.
        let client = Write::builder()
            .with_endpoint(endpoint)
            .with_credentials(Anonymous::new().build())
            .build()
            .await?;
        let writer = client
            .open_default_stream(TABLE)
            .build_proto(Row::schema())
            .await?;
        let rows = ProtoRows::new().set_serialized_rows([sample_row()?.to_row()?]);

        // Appends to the default stream succeed without an offset.
        let success = mock_v1::AppendRowsResponse {
            response: Some(Response::AppendResult(AppendResult { offset: None })),
            ..Default::default()
        };
        response_tx.send(Ok(success)).await?;
        let response = writer.append(rows).send().await?;
        assert_eq!(response.offset, None);

        // The mock received the schema and the bytes we expect.
        let mut requests = requests_rx.await?;
        let request = requests
            .recv()
            .await
            .expect("the mock received a request")?;
        assert_eq!(request.write_stream, format!("{TABLE}/streams/_default"));
        let data = match request.rows {
            Some(Rows::ProtoRows(data)) => data,
            other => panic!("expected proto rows, got {other:?}"),
        };
        let schema = data.writer_schema.and_then(|s| s.proto_descriptor);
        assert_eq!(schema, Some(sent_descriptor()?));
        let rows = data.rows.map(|r| r.serialized_rows);
        assert_eq!(rows, Some(vec![sample_prost_row().encode_to_vec()]));
        Ok(())
    }
}
