use std::{
    any::{Any, TypeId},
    collections::{BTreeMap, btree_map::Iter},
    ops::Deref,
    sync::Arc,
};

use crate::{cast::DowncastArc, sync::types::ArcMutex};

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
    storage: ArcMutex<BTreeMap<TypeId, SendableMultiTypeDictItem<dyn Any + Send + Sync + 'static>>>,
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
            storage: Default::default(),
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

        let old_item = self.storage.lock().insert(type_id, new_item.clone());

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
        let type_id = TypeId::of::<ItemType>();

        let result = self
            .storage
            .lock()
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
        self.storage.lock().get(&type_id).cloned()
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
        self.storage.lock().remove(&type_id)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

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
