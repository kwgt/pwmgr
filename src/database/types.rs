/*
 * Password manager
 *
 *  Copyright (C) 2025 HIroshi Kuwagata
 */

//!
//! データベースに登録する型を定義するモジュール
//!

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::ops::{Deref, RangeInclusive};

use anyhow::{Error, Result};
use redb::{Key, TypeName, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de;
use ulid::{DecodeError, Ulid};

///
/// サービスIDを表す構造体
///
#[derive(Clone, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub(crate) struct ServiceId(Ulid);

impl ServiceId {
    ///
    /// サービスIDオブジェクトの生成
    ///
    pub(crate) fn new() -> Self {
        Self(Ulid::new())
    }

    ///
    /// 文字列からの変換
    ///
    /// # 引数
    /// * `s` - 変換対象の文字列
    ///
    /// # 戻り値
    /// 変換に成功した場合は、サービスIDオブジェクトを`Ok()`でラップして返す。失
    /// 敗した場合はエラー情報を`Err()`でラップして返す。
    ///
    pub(crate) fn from_string(s: &str) -> Result<Self, DecodeError> {
        Ulid::from_string(s).map(Self)
    }

    ///
    /// サービスIDの全域を表す範囲オブジェクトを返す
    ///
    pub(crate) fn range_all() -> RangeInclusive<ServiceId> {
        Self::min()..=Self::max()
    }

    ///
    /// サービスIDの最小値を返す
    ///
    pub(crate) fn min() -> Self {
        Self::from_string("00000000000000000000000000")
            .expect("invalid ULID string")
    }

    ///
    /// サービスIDの最大値を返す
    ///
    pub(crate) fn max() -> Self {
        Self::from_string("7ZZZZZZZZZZZZZZZZZZZZZZZZZ")
            .expect("invalid ULID string")
    }
}

// Derefトレイトの実装
impl Deref for ServiceId {
    type Target = Ulid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

// TryFromトレイトの実装
impl TryFrom<&str> for ServiceId {
    type Error = Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match Ulid::from_string(value) {
            Ok(ulid) => Ok(Self(ulid)),
            Err(err) => Err(err.into()),
        }
    }
}

// Fromトレイトの実装
impl From<&Ulid> for ServiceId {
    fn from(value: &Ulid) -> Self {
        Self(value.to_owned())
    }
}

// Valueトレイトの実装
impl Value for ServiceId {
    type SelfType<'a> = ServiceId;
    type AsBytes<'a> = [u8; 16];

    fn fixed_width() -> Option<usize> {
        Some(16)
    }

    fn type_name() -> TypeName {
        TypeName::new("ServiceId")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b
    {
        value.to_bytes()
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a
    {
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(data);

        Self(Ulid::from_bytes(bytes))
    }
}

// Keyトレイトの実装
impl Key for ServiceId {
    fn compare(a: &[u8], b: &[u8]) -> Ordering {
        a.cmp(b)
    }
}

// Serializeトレイトの実装
impl Serialize for ServiceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            serializer.serialize_str(&self.0.to_string())
        } else {
            serializer.serialize_bytes(&self.0.to_bytes())
        }
    }
}

// Deserializeトレイトの実装
impl<'de> Deserialize<'de> for ServiceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let string = String::deserialize(deserializer)?;
            Ulid::from_string(&string)
                .map(ServiceId)
                .map_err(de::Error::custom)
        } else {
            Ok(ServiceId(Ulid::from_bytes(
               <[u8; 16]>::deserialize(deserializer)?
            )))
        }
    }
}

// Displayトレイトの実装
impl fmt::Display for ServiceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.to_string())
    }
}

///
///
/// サービスエントリの定義
///
#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Entry {
    /// サービスのID
    id: ServiceId,

    /// サービス名
    service: String,

    /// サービス名の別名のリスト(サービスの旧名など)
    aliases: Vec<String>,

    /// エントリに付与されたタグのリスト
    tags: Vec<String>,

    /// エントリのプロパティ
    properties: BTreeMap<String, String>,
}

impl Entry {
    ///
    /// 新規登録用のエントリ生成
    ///
    /// # 引数
    /// * `id` - サービスID
    /// * `service` - サービス名
    /// * `aliased` - 別名のリスト
    /// * `tags` - タグのリスト
    /// * `properties` - プロパティ
    ///
    /// # 戻り値
    /// 初期化済みのエントリオブジェクト
    ///
    pub(crate) fn new(
        id: ServiceId,
        service: String,
        mut aliases: Vec<String>,
        mut tags: Vec<String>,
        properties: BTreeMap<String, String>,
    ) -> Self {
        aliases.sort();
        aliases.dedup();

        tags.sort();
        tags.dedup();

        Self {
            id,
            service,
            aliases,
            tags,
            properties,
        }
    }

    ///
    /// サービスIDへのアクセサ
    ///
    /// # 戻り値
    /// サービスIDオブジェクトを返す
    ///
    pub(crate) fn id(&self) -> ServiceId {
        self.id.clone()
    }

    ///
    /// サービス名へのアクセサ
    ///
    /// # 戻り値
    /// サービス名を返す
    ///
    pub(crate) fn service(&self) -> String {
        self.service.clone()
    }

    ///
    /// タグリストへのアクセサ
    ///
    /// # 戻り値
    /// タグのリストを格納した`Vec<String>`オブジェクトを返す。
    ///
    pub(crate) fn tags(&self) -> Vec<String> {
        self.tags.clone()
    }

    ///
    /// 別名リストへのアクセサ
    ///
    pub(crate) fn aliases(&self) -> Vec<String> {
        self.aliases.clone()
    }

    ///
    /// プロパティへのアクセサ
    ///
    pub(crate) fn properties(&self) -> BTreeMap<String, String> {
        self.properties.clone()
    }
}

// Valueトレイトの実装
impl Value for Entry {
    type SelfType<'a> = Entry;
    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn type_name() -> TypeName {
        TypeName::new("Entry")
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a
    {
        rmp_serde::from_slice::<Entry>(data)
            .expect("invalid MessagePack packed bytes")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b
    {
        rmp_serde::to_vec(value)
            .expect("failed to serialize to MessagePack bytes")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_id_roundtrip_string() {
        let id = ServiceId::new();
        let s = id.to_string();
        let back = ServiceId::from_string(&s).unwrap();
        assert_eq!(id, back);
    }

    ///
    /// min < new() < max の順序性をざっくり確認
    ///
    #[test]
    fn service_id_ordering() {
        let min = ServiceId::min();
        let max = ServiceId::max();
        let mid = ServiceId::new();
        assert!(min < mid && mid < max);
    }

    ///
    /// Entry::new が別名/タグをソート＋重複排除することを確認
    ///
    #[test]
    fn entry_new_dedup_sorts() {
        let id = ServiceId::new();
        let entry = Entry::new(
            id.clone(),
            "svc".to_string(),
            vec!["b".into(), "a".into(), "a".into()],
            vec!["tag2".into(), "tag1".into(), "tag1".into()],
            BTreeMap::new(),
        );

        // aliases/tags がソート済みかつ重複除去されていること
        assert_eq!(entry.id(), id);
        assert_eq!(entry.service(), "svc".to_string());
        assert_eq!(entry.aliases(), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(entry.tags(), vec!["tag1".to_string(), "tag2".to_string()]);
    }

    ///
    /// 各アクセサがクローンを返すことを確認（ミュータブル参照でない）
    ///
    #[test]
    fn entry_accessors_clone() {
        let id = ServiceId::new();
        let mut props = BTreeMap::new();
        props.insert("k".to_string(), "v".to_string());
        let entry = Entry::new(
            id.clone(),
            "svc".to_string(),
            vec!["a".into()],
            vec!["tag".into()],
            props.clone(),
        );

        // 値が一致していること（同一オブジェクト参照ではなくクローンが返る前提）
        assert_eq!(entry.id(), id);
        assert_eq!(entry.service(), "svc".to_string());
        assert_eq!(entry.aliases(), vec!["a".to_string()]);
        assert_eq!(entry.tags(), vec!["tag".to_string()]);
        assert_eq!(entry.properties(), props);
    }

    ///
    /// Value実装のバイト往復が同一IDを再現すること
    ///
    #[test]
    fn service_id_value_bytes_roundtrip() {
        let id = ServiceId::new();
        let bytes = ServiceId::as_bytes(&id);
        let back = ServiceId::from_bytes(&bytes);
        assert_eq!(id, back);
    }
}
