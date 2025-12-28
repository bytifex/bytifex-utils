use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, btree_map::Iter},
    ops::Deref,
    sync::Arc,
};

use parking_lot::{Condvar, Mutex};

use crate::{
    cast::DowncastArc,
    sync::types::{ArcMutex, arc_mutex_new},
};

type ItemTypeLock = Arc<(Mutex<bool>, Condvar)>;

pub struct SendableMultiTypeDictItem<ItemType: ?Sized> {
    type_id: TypeId,
    item: Arc<ItemType>,
}

impl<ItemType: ?Sized> Clone for SendableMultiTypeDictItem<ItemType> {
    fn clone(&self) -> Self {
        Self {
            type_id: self.type_id,
            item: self.item.clone(),
        }
    }
}

pub struct SendableMultiTypeDict {
    storage: BTreeMap<TypeId, SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>>,
    item_type_locks: ArcMutex<BTreeMap<TypeId, ItemTypeLock>>,
}

pub struct ItemTypeGuard {
    item_type_locks: ArcMutex<BTreeMap<TypeId, ItemTypeLock>>,
    type_id: TypeId,
    lock: Arc<(Mutex<bool>, Condvar)>,
}

pub struct SendableMultiTypeDictIterator<'a> {
    inner_iterator: Iter<'a, TypeId, SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>>,
}

pub struct SendableMultiTypeDictInsertResult<ItemType: ?Sized> {
    pub new_item: SendableMultiTypeDictItem<ItemType>,
    pub old_item: Option<SendableMultiTypeDictItem<ItemType>>,
}

impl<'a> Iterator for SendableMultiTypeDictIterator<'a> {
    type Item = SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner_iterator.next().map(|value| value.1.clone())
    }
}

impl SendableMultiTypeDict {
    pub fn new() -> Self {
        Self {
            storage: BTreeMap::new(),
            item_type_locks: arc_mutex_new(BTreeMap::new()),
        }
    }

    pub fn insert<ItemType>(
        &mut self,
        item: ItemType,
    ) -> SendableMultiTypeDictInsertResult<ItemType>
    where
        ItemType: Any + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<ItemType>();

        let result = self.insert_any(item, type_id);
        if let Some(new_item) = result.new_item.downcast() {
            if let Some(old_item) = result.old_item {
                if let Some(old_item) = old_item.downcast() {
                    SendableMultiTypeDictInsertResult {
                        new_item,
                        old_item: Some(old_item),
                    }
                } else {
                    unreachable!();
                }
            } else {
                SendableMultiTypeDictInsertResult {
                    new_item,
                    old_item: None,
                }
            }
        } else {
            unreachable!();
        }
    }

    pub fn insert_any(
        &mut self,
        item: impl Any + Send + Sync + 'static,
        type_id: TypeId,
    ) -> SendableMultiTypeDictInsertResult<dyn Any + Send + Sync + 'static> {
        let new_item: SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static> =
            SendableMultiTypeDictItem {
                type_id,
                item: Arc::new(item),
            };

        let old_item = self.storage.insert(type_id, new_item.clone());

        SendableMultiTypeDictInsertResult { new_item, old_item }
    }

    pub fn get_item_ref<ItemType>(&self) -> Option<SendableMultiTypeDictItem<ItemType>>
    where
        ItemType: Any + Send + Sync,
    {
        let type_id = TypeId::of::<ItemType>();

        self.get_item_ref_any(type_id)
            .and_then(|item| item.downcast::<ItemType>())
    }

    pub fn get_or_insert_item_ref<ItemType>(
        &mut self,
        item_creator: impl FnOnce() -> ItemType,
    ) -> SendableMultiTypeDictItem<ItemType>
    where
        ItemType: Any + Send + Sync + 'static,
    {
        let _item_type_guard = self.lock_item_type::<ItemType>();

        let type_id = TypeId::of::<ItemType>();

        let result = self
            .storage
            .entry(type_id)
            .or_insert_with(|| SendableMultiTypeDictItem {
                type_id,
                item: Arc::new(item_creator()),
            })
            .clone()
            .downcast::<ItemType>();

        if let Some(item) = result {
            item
        } else {
            unreachable!()
        }
    }

    pub fn get_item_ref_any(
        &self,
        type_id: TypeId,
    ) -> Option<SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>> {
        self.storage.get(&type_id).cloned()
    }

    pub fn remove<ItemType>(&mut self) -> Option<Arc<ItemType>>
    where
        ItemType: Any + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<ItemType>();

        self.remove_by_type_id(type_id)
            .and_then(|item| item.downcast::<ItemType>())
            .map(|item| item.as_arc_ref().clone())
    }

    pub fn remove_by_type_id(
        &mut self,
        type_id: TypeId,
    ) -> Option<SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>> {
        self.storage.remove(&type_id)
    }

    pub fn iter(&self) -> SendableMultiTypeDictIterator<'_> {
        SendableMultiTypeDictIterator {
            inner_iterator: self.storage.iter(),
        }
    }

    fn lock_item_type<ItemType>(&self) -> ItemTypeGuard
    where
        ItemType: Any + Send + Sync + 'static,
    {
        let type_id = TypeId::of::<ItemType>();
        let mut item_type_locks = self.item_type_locks.lock();
        let entry = item_type_locks
            .entry(type_id)
            .or_insert_with(|| Arc::new((Mutex::new(false), Condvar::new())));

        let entry = entry.clone();

        drop(item_type_locks);

        let mut item_type_locked = entry.0.lock();
        while *item_type_locked {
            entry.1.wait(&mut item_type_locked);
        }
        *item_type_locked = true;

        ItemTypeGuard {
            item_type_locks: self.item_type_locks.clone(),
            type_id,
            lock: entry.clone(),
        }
    }
}

impl SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static> {
    pub fn downcast<CastType: 'static>(&self) -> Option<SendableMultiTypeDictItem<CastType>> {
        self.item
            .downcast_arc::<CastType>()
            .map(|item| SendableMultiTypeDictItem {
                type_id: self.type_id,
                item,
            })
    }
}

impl<ItemType: ?Sized> SendableMultiTypeDictItem<ItemType> {
    pub fn as_arc_ref(&self) -> &Arc<ItemType> {
        &self.item
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }
}

impl<ItemType: ?Sized> Deref for SendableMultiTypeDictItem<ItemType> {
    type Target = ItemType;

    fn deref(&self) -> &Self::Target {
        self.as_arc_ref()
    }
}

impl Default for SendableMultiTypeDict {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ItemTypeGuard {
    fn drop(&mut self) {
        let mut item_type_locks = self.item_type_locks.lock();
        // check that only item_type_locks and self contains this lock
        if Arc::strong_count(&self.lock) == 2 {
            // nobody tries to lock the item type
            item_type_locks.remove(&self.type_id);
        } else {
            // somebody tries to lock the item type
            let mut item_type_locked = self.lock.0.lock();
            *item_type_locked = false;
            self.lock.1.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{any::Any, sync::Arc};

    use crate::containers::sendable_multi_type_dict::SendableMultiTypeDictItem;

    use super::SendableMultiTypeDict;

    #[derive(Debug, Eq, PartialEq)]
    struct A {
        value: String,
    }

    #[derive(Debug, Eq, PartialEq)]
    struct B {
        value: String,
    }

    #[test]
    fn store_and_remove() {
        let mut dict = SendableMultiTypeDict::new();

        assert!(
            dict.insert(A {
                value: "A0".to_string(),
            })
            .old_item
            .is_none()
        );

        assert_eq!(
            *dict
                .insert(A {
                    value: "A1".to_string(),
                })
                .old_item
                .unwrap()
                .as_arc_ref(),
            Arc::new(A {
                value: "A0".to_string(),
            })
        );

        assert!(
            dict.insert(B {
                value: "B".to_string(),
            })
            .old_item
            .is_none()
        );

        assert_eq!(
            *dict.get_item_ref::<A>().unwrap().as_arc_ref(),
            Arc::new(A {
                value: "A1".to_string(),
            })
        );

        assert_eq!(
            *dict.get_item_ref::<B>().unwrap().as_arc_ref(),
            Arc::new(B {
                value: "B".to_string(),
            })
        );

        let systems: Vec<SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>> =
            dict.iter().collect();
        assert_eq!(systems.len(), 2);
        if systems[0].downcast::<A>().is_some() {
            assert!(systems[0].downcast::<A>().is_some());
            assert!(systems[1].downcast::<B>().is_some());
        } else {
            assert!(systems[0].downcast::<B>().is_some());
            assert!(systems[1].downcast::<A>().is_some());
        }

        assert_eq!(
            *dict.remove::<A>().unwrap(),
            A {
                value: "A1".to_string(),
            }
        );

        assert!(dict.get_item_ref::<A>().is_none());

        assert_eq!(
            *dict.remove::<B>().unwrap(),
            B {
                value: "B".to_string(),
            }
        );

        assert!(dict.get_item_ref::<B>().is_none());
    }
}
