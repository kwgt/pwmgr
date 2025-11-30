/*
 * Password manager
 *
 *  Copyright (C) 2025 HIroshi Kuwagata
 */

//!
//! データベース関連処理をまとめたモジュール
//!

pub(crate) mod types;

use std::collections::HashMap;
use std::marker::PhantomData;
use std::path::Path;

use anyhow::Result;
use redb::{
    Database, MultimapTableDefinition, Range, ReadableDatabase, ReadableTable,
    StorageError, TableDefinition, WriteTransaction
};

use crate::database::types::{Entry, ServiceId};

/// エントリ登録テーブル
static ENTRIES_TABLE: TableDefinition<ServiceId, Entry> =
    TableDefinition::new("entries");

/// タグ管理テーブル
static TAGS_TABLE: MultimapTableDefinition<String, ServiceId> =
    MultimapTableDefinition::new("tags");

///
/// 2つのベクタの差分（aにのみ含まれる要素）を返す。差分が空ならNone。
///
fn vec_diff<T>(a: &Vec<T>, b: &Vec<T>) -> Option<Vec<T>>
where 
    T: PartialEq + Clone,
{
    let diff: Vec<T> = a.iter()
        .filter(|val| !b.contains(val))
        .cloned()
        .collect();

    (!diff.is_empty()).then_some(diff)
}

///
/// タグリストから指定IDを削除する。
///
/// # 引数
/// * `tnx` - 書き込みトランザクション
/// * `id` - 削除対象のサービスID
/// * `tags` - 削除対象タグのリスト
///
fn shrink_tag_list(tnx: &WriteTransaction, id: &ServiceId, tags: Vec<String>)
    -> Result<()>
{
    let mut table = tnx.open_multimap_table(TAGS_TABLE)?;

    for tag in tags {
        // タグに対応するIDを削除
        table.remove(&tag, id)?;
    }

    Ok(())
}

///
/// タグリストに指定IDを追加する。
///
/// # 引数
/// * `tnx` - 書き込みトランザクション
/// * `id` - 追加するサービスID
/// * `tags` - 追加対象タグのリスト
///
fn expand_tag_list(tnx: &WriteTransaction, id: &ServiceId, tags: Vec<String>)
    -> Result<()>
{
    let mut table = tnx.open_multimap_table(TAGS_TABLE)?;

    for tag in tags {
        // タグに対応するIDを追加
        table.insert(&tag, id)?;
    }

    Ok(())
}

///
/// サービスID群取得のためのイテレータ
///
#[allow(dead_code)]
struct ServiceIdIter<'a> {
    /// DBに対するレンジオブジェクト
    inner: Range<'a, ServiceId, Entry>,

    /// マーカオブジェクト
    _marker: PhantomData<Entry>,
}

// Iteratorの実装
impl<'a> Iterator for ServiceIdIter<'a> {
    type Item = Result<ServiceId>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.inner.next()? {
            Ok((id, _)) => Some(Ok(id.value())),
            Err(err) => Some(Err(err.into())),
        }
    }
}

///
/// エントリ操作手順を集約する構造体
///
pub(crate) struct EntryManager {
    db: Database,
}

impl EntryManager {
    ///
    /// エントリーマネージャのオープン
    ///
    /// # 引数
    /// * `path` - データベースファイルへのパス
    ///
    /// # 戻り値
    /// データベースのオープンに成功した場合はエントリーマネージャオブジェクトを
    /// `Ok()`でラップして返す。失敗した場合はエラー情報を `Err()`でラップして返
    /// す。
    ///
    pub(crate) fn open<P>(path: P) -> Result<Self> 
    where
        P: AsRef<Path>
    {
        let db = match Database::create(path) {
            Ok(db) => db,
            Err(err) => return Err(err.into()),

        };

        Ok(Self {db})
    }

    ///
    /// エントリーの書き込み
    ///
    /// # 引数
    /// * `entry` - データベースへ書き込みエントリオブジェクト
    ///
    /// # 戻り値
    /// 書き込みに成功した場合は`Ok(())`を、失敗した場合はエラー情報を `Err()`で
    /// ラップして返す。
    ///
    pub(crate) fn put(&mut self, entry: &Entry) -> Result<()> {
        /*
         * トランザクションを開始
         */
        let tnx = self.db.begin_write()?;

        {
            let id = entry.id();
            let mut table = tnx.open_table(ENTRIES_TABLE)?;

            /*
             * タグテーブルを更新
             */
            if let Some(existing) = table.get(&id)? {
                /*
                 * 既存タグが存在する場合
                 */

                let a = existing.value().tags();
                let b = entry.tags();

                // 既存エントリにのみに存在するタグがある場合は、そのタグに対応
                // するタグリストからエントリのサービスIDを削除
                if let Some(diff) = vec_diff(&a, &b) {
                    shrink_tag_list(&tnx, &id, diff)?;
                }

                // 新規エントリにのみに存在するタグがある場合は、そのタグに対応
                // するに対応するタグリストにエントリのサービスIDを追加
                if let Some(diff) = vec_diff(&b, &a) {
                    expand_tag_list(&tnx, &id, diff)?;
                }

            } else {
                /*
                 * 既存タグが存在しない場合
                 */

                // 新規エントリの持つタグに対応するタグリストにエントリのサービ
                // スIDを追加
                expand_tag_list(&tnx, &id, entry.tags())?;
            }


            /*
             * 新規エントリを登録する
             */
            table.insert(&id, entry)?;
        }

        /*
         * トランザクションをコミット
         */
        tnx.commit()?;

        Ok(())
    }

    ///
    /// エントリーの取得
    ///
    /// # 引数
    /// * `id` - 取得するエントリのサービスID
    ///
    /// # 戻り値
    /// 取得に成功した場合はエントリ情報を`Ok()`でラップして返す。失敗した場合は
    /// エラー情報を `Err()`でラップして返す。
    ///
    pub(crate) fn get(&mut self, id: &ServiceId) -> Result<Option<Entry>> {
        /*
         * トランザクションを開始
         */
        let tnx = self.db.begin_read()?;

        {
            let table = tnx.open_table(ENTRIES_TABLE)?;

            Ok(table.get(id)?.map(|entry| entry.value()))
        }
    }

    ///
    /// エントリーの削除
    ///
    /// # 引数
    /// * `id` - 削除対象のサービスのID
    ///
    /// # 戻り値
    /// 削除に成功した場合は`Ok(())`を、失敗した場合はエラー情報を `Err()`でラッ
    /// プして返す。
    ///
    pub(crate) fn remove(&mut self, id: &ServiceId) -> Result<()> {
        /*
         * トランザクションを開始
         */
        let tnx = self.db.begin_write()?;

        {
            let mut table = tnx.open_table(ENTRIES_TABLE)?;

            /*
             * タグリストを更新
             */
            if let Some(entry) = table.get(id)? {
                // エントリが存在する場合はエントリの持つタグに対応するタグリス
                // トからサービスIDを削除
                shrink_tag_list(&tnx, &id, entry.value().tags())?;
            } else {
                // エントリが無い場合は、何も行わないのでリターン
                return Ok(())
            }

            // エントリテーブルからエントリを削除
            table.remove(id)?;
        }

        /*
         * トランザクションをコミット
         */
        tnx.commit()?;

        Ok(())
    }

    ///
    /// 全サービスのIDのリストの取得
    ///
    /// # 戻り値
    /// 取得に成功した場合はサービスIDのリストを`Ok()`でラップして返す。
    ///
    pub(crate) fn all_service(&self) -> Result<Vec<ServiceId>> {
        /*
         * トランザクションを開始
         */
        let tnx = self.db.begin_read()?;

        {
            let table = tnx.open_table(ENTRIES_TABLE)?;

            table.range(ServiceId::range_all())?
                .into_iter()
                .map(|res| res.map(|(id, _)| id.value()))
                .collect::<redb::Result<Vec<ServiceId>, StorageError>>()
                .map_err(|err| err.into())

        }
    }

    ///
    /// 全タグと件数の一覧を取得
    ///
    pub(crate) fn all_tags(&mut self) -> Result<Vec<(String, usize)>> {
        let mut counts: HashMap<String, usize> = HashMap::new();

        for id in self.all_service()? {
            if let Some(entry) = self.get(&id)? {
                for tag in entry.tags() {
                    *counts.entry(tag).or_insert(0) += 1;
                }
            }
        }

        Ok(counts.into_iter().collect())
    }

    ///
    /// タグに紐づくサービスIDの一覧を取得
    ///
    /// # 引数
    /// * `tag` - 一覧を取得するタグ
    ///
    /// # 返り値
    /// 取得に成功した場合はサービスIDのリストを`Ok()`でラップして返す。
    ///
    pub(crate) fn tagged_service(&self, tag: &str)
        -> Result<Vec<ServiceId>>
    {
        /*
         * トランザクションの開始
         */
        let tnx = self.db.begin_read()?;

        let table = tnx.open_multimap_table(TAGS_TABLE)?;


        table.get(&tag.to_string())?
            .map(|id| id.map(|id| id.value()))
            .collect::<redb::Result<Vec<ServiceId>, StorageError>>()
            .map_err(|err| err.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use ulid::Ulid;

    use crate::database::types::{Entry, ServiceId};

    ///
    /// テスト用の一時DBファイルパスを生成
    ///
    fn temp_db_path() -> PathBuf {
        std::env::temp_dir().join(format!("pwmgr-test-{}.redb", Ulid::new()))
    }

    ///
    /// 簡易エントリ生成ヘルパ
    ///
    fn make_entry(id: ServiceId, service: &str, aliases: &[&str], tags: &[&str]) -> Entry {
        Entry::new(
            id,
            service.to_string(),
            aliases.iter().map(|s| s.to_string()).collect(),
            tags.iter().map(|s| s.to_string()).collect(),
            BTreeMap::new(),
        )
    }

    ///
    /// 追加→取得→タグ検索の基本動作を確認
    ///
    #[test]
    fn put_then_get_and_tagged() {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(&path).unwrap();
        let id = ServiceId::new();
        let entry = make_entry(id.clone(), "svc1", &["alias"], &["tag1"]);

        mgr.put(&entry).unwrap();

        let got = mgr.get(&id).unwrap().unwrap();
        assert_eq!(got.service(), "svc1".to_string());

        let mut tagged = mgr.tagged_service("tag1").unwrap();
        tagged.sort();
        assert_eq!(tagged, vec![id.clone()]);
    }

    ///
    /// タグ更新でマルチマップが差分反映されることを確認
    ///
    #[test]
    fn update_tags_updates_multimap() {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(&path).unwrap();
        let id = ServiceId::new();

        let entry1 = make_entry(id.clone(), "svc", &[], &["tag1", "tag2"]);
        mgr.put(&entry1).unwrap();

        let entry2 = make_entry(id.clone(), "svc", &[], &["tag2", "tag3"]);
        mgr.put(&entry2).unwrap();

        // 旧タグ(tag1)からは消え、新タグ(tag3)に追加されていること
        assert!(!mgr.tagged_service("tag1").unwrap().contains(&id));
        assert!(mgr.tagged_service("tag2").unwrap().contains(&id));
        assert!(mgr.tagged_service("tag3").unwrap().contains(&id));
    }

    ///
    /// remove で entries/tags 両方からエントリが消えること
    ///
    #[test]
    fn remove_cleans_tags() {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(&path).unwrap();
        let id = ServiceId::new();
        let entry = make_entry(id.clone(), "svc", &[], &["tag1"]);

        mgr.put(&entry).unwrap();
        mgr.remove(&id).unwrap();

        assert!(mgr.get(&id).unwrap().is_none());
        assert!(!mgr.tagged_service("tag1").unwrap().contains(&id));
    }

    ///
    /// all_service が登録済みIDをすべて返すこと
    ///
    #[test]
    fn all_service_lists_all_ids() {
        let path = temp_db_path();
        let mut mgr = EntryManager::open(&path).unwrap();
        let id1 = ServiceId::new();
        let id2 = ServiceId::new();

        mgr.put(&make_entry(id1.clone(), "a", &[], &[])).unwrap();
        mgr.put(&make_entry(id2.clone(), "b", &[], &[])).unwrap();

        let mut all = mgr.all_service().unwrap();
        all.sort();

        let mut expected = vec![id1, id2];
        expected.sort();

        assert_eq!(all, expected);
    }
}
