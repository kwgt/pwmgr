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
use std::fmt::{Display, Formatter};
use std::ops::{Deref, RangeInclusive};

use chrono::{DateTime, Duration, Local};
use anyhow::{Error, Result};
use redb::{Key, TypeName, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de;
use ulid::{DecodeError, Ulid};

// ローカルモジュール: last_update の人間可読シリアライズ
mod serde_human_datetime {
    use chrono::{DateTime, Local};
    use serde::{Deserialize, Deserializer, Serializer};

    /// Option<DateTime<Local>> を RFC3339 文字列でシリアライズする
    pub(crate) fn serialize<S>(
        val: &Option<DateTime<Local>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match val {
            Some(dt) => serializer.serialize_str(&dt.to_rfc3339()),
            None => serializer.serialize_none(),
        }
    }

    /// RFC3339 文字列（または None）から Option<DateTime<Local>> を復元する
    pub(crate) fn deserialize<'de, D>(deserializer: D)
        -> Result<Option<DateTime<Local>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| Some(dt.with_timezone(&Local)))
                .map_err(serde::de::Error::custom),
            None => Ok(None),
        }
    }
}

/// 現在時刻（ローカル）を秒精度に丸めて返す
fn now_sec() -> DateTime<Local> {
    let now = Local::now();
    now - Duration::nanoseconds(now.timestamp_subsec_nanos() as i64)
}

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

// Intoトレイトの実装
impl Into<String> for ServiceId {
    fn into(self) -> String {
        self.0.to_string()
    }
}

// Displayトレイトの実装
impl Display for ServiceId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.to_string())
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

    /// 最終更新日時（ローカル時間、ISO8601文字列でシリアライズ）
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        serialize_with = "serde_human_datetime::serialize",
        deserialize_with = "serde_human_datetime::deserialize"
    )]
    last_update: Option<DateTime<Local>>,

    /// ソフトリムーブフラグ
    #[serde(default, skip_serializing_if = "Option::is_none")]
    removed: Option<bool>,
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
            removed: None,
            last_update: Some(now_sec()),
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

    ///
    /// ソフトリムーブフラグへのアクセサ
    ///
    pub(crate) fn is_removed(&self) -> bool {
        self.removed.unwrap_or(false)
    }

    ///
    /// 最終更新日時へのアクセサ
    ///
    pub(crate) fn last_update(&self) -> Option<DateTime<Local>> {
        self.last_update
    }

    ///
    /// 最終更新日時を現在時刻で更新
    ///
    pub(crate) fn set_last_update_now(&mut self) {
        self.last_update = Some(now_sec());
    }

    ///
    /// 最終更新日時を任意の値で設定（テスト用など）
    ///
    #[allow(dead_code)]
    pub(crate) fn set_last_update(&mut self, dt: DateTime<Local>) {
        self.last_update = Some(dt);
    }

    ///
    /// ソフトリムーブフラグを設定
    ///
    pub(crate) fn set_removed(&mut self, removed: bool) {
        self.removed = removed.then_some(true);
    }

    ///
    /// 秘匿項目をマスク表示用に上書きする
    ///
    pub(crate) fn mask_secret_properties(&mut self) {
        for (key, value) in self.properties.iter_mut() {
            if key.ends_with('!') {
                *value = "<< SECRET >>".to_string();
            }
        }
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
        rmp_serde::to_vec_named(value)
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
    /// mask_secret_properties が秘匿項目の値を隠蔽することを確認
    ///
    #[test]
    fn entry_mask_secret_properties_masks_values() {
        let id = ServiceId::new();
        let mut props = BTreeMap::new();
        props.insert("user".to_string(), "alice".to_string());
        props.insert("password!".to_string(), "secret".to_string());

        let mut entry = Entry::new(
            id.clone(),
            "svc".to_string(),
            vec!["a".into()],
            vec!["tag".into()],
            props,
        );

        entry.mask_secret_properties();

        let properties = entry.properties();
        assert_eq!(properties.get("user"), Some(&"alice".to_string()));
        assert_eq!(
            properties.get("password!"),
            Some(&"<< SECRET >>".to_string()
        ));
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
