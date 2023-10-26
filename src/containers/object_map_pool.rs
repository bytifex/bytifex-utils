use std::collections::BTreeMap;

use super::object_pool::{ObjectPool, ObjectPoolIndex, ObjectPoolIter, ObjectPoolIterMut};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct ObjectMapPoolIndex(ObjectPoolIndex);

impl ObjectMapPoolIndex {
    pub fn invalid() -> Self {
        Self(ObjectPoolIndex::invalid())
    }

    pub fn invalidate(&mut self) -> Self {
        let mut id = Self::invalid();
        std::mem::swap(&mut id, self);

        id
    }
}

pub struct ObjectMapPool<KeyType, ValueType>
where
    KeyType: Clone + Ord,
{
    object_pool: ObjectPool<(KeyType, ValueType)>,
    map_of_indices: BTreeMap<KeyType, ObjectMapPoolIndex>,
}

impl<KeyType, ValueType> ObjectMapPool<KeyType, ValueType>
where
    KeyType: Clone + Ord,
{
    pub fn new() -> Self {
        Self {
            object_pool: ObjectPool::new(),
            map_of_indices: BTreeMap::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            object_pool: ObjectPool::with_capacity(capacity),
            map_of_indices: BTreeMap::new(),
        }
    }

    pub fn create_object(&mut self, key: KeyType, value: ValueType) -> ObjectMapPoolIndex {
        let index = self.object_pool.create_object((key.clone(), value));
        self.map_of_indices.insert(key, ObjectMapPoolIndex(index));
        ObjectMapPoolIndex(index)
    }

    pub fn release_object_by_index(
        &mut self,
        index: ObjectMapPoolIndex,
    ) -> Option<(KeyType, ValueType)> {
        self.object_pool.release_object(index.0).map(|object| {
            self.map_of_indices.remove(&object.0);
            (object.0, object.1)
        })
    }

    pub fn release_object_by_key(&mut self, key: &KeyType) -> Option<(KeyType, ValueType)> {
        match self.map_of_indices.get(key) {
            Some(index) => self.release_object_by_index(*index),
            None => None,
        }
    }

    pub fn get_ref_by_index(&self, index: ObjectMapPoolIndex) -> Option<(&KeyType, &ValueType)> {
        self.object_pool
            .get_ref(index.0)
            .map(|(key, value)| (key, value))
    }

    pub fn get_ref_by_key(&self, key: &KeyType) -> Option<(&KeyType, &ValueType)> {
        self.map_of_indices
            .get(key)
            .map(|index| match self.get_ref_by_index(*index) {
                Some(pair) => pair,
                None => unreachable!(),
            })
    }

    pub fn get_mut_by_index(
        &mut self,
        index: ObjectMapPoolIndex,
    ) -> Option<(&KeyType, &mut ValueType)> {
        self.object_pool
            .get_mut(index.0)
            .map(|(key, value)| (&*key, value))
    }

    pub fn get_mut_by_key(&mut self, key: &KeyType) -> Option<(&KeyType, &mut ValueType)> {
        match self.map_of_indices.get_mut(key).map(|index| *index) {
            Some(index) => self.get_mut_by_index(index),
            None => None,
        }
    }

    pub fn iter(&self) -> ObjectMapPoolIter<'_, KeyType, ValueType> {
        ObjectMapPoolIter {
            inner_iterator: self.object_pool.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> ObjectMapPoolIterMut<'_, KeyType, ValueType> {
        ObjectMapPoolIterMut {
            inner_iterator: self.object_pool.iter_mut(),
        }
    }

    pub fn len(&self) -> usize {
        self.object_pool.len()
    }

    pub fn is_empty(&self) -> bool {
        self.object_pool.is_empty()
    }

    pub fn first_index(
        &self,
        pred: impl Fn(&KeyType, &ValueType) -> bool,
    ) -> Option<ObjectMapPoolIndex> {
        self.object_pool
            .first_index(|(key, value)| pred(key, value))
            .map(ObjectMapPoolIndex)
    }
}

impl<KeyType, ValueType> Default for ObjectMapPool<KeyType, ValueType>
where
    KeyType: Clone + Ord,
{
    fn default() -> Self {
        Self::new()
    }
}

pub struct ObjectMapPoolIter<'a, KeyType, ValueType> {
    inner_iterator: ObjectPoolIter<'a, (KeyType, ValueType)>,
}

impl<'a, KeyType, ValueType> Iterator for ObjectMapPoolIter<'a, KeyType, ValueType> {
    type Item = (&'a KeyType, &'a ValueType);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner_iterator.next().map(|(key, value)| (key, value))
    }
}

pub struct ObjectMapPoolIterMut<'a, KeyType, ValueType> {
    inner_iterator: ObjectPoolIterMut<'a, (KeyType, ValueType)>,
}

impl<'a, KeyType, ValueType> Iterator for ObjectMapPoolIterMut<'a, KeyType, ValueType> {
    type Item = (&'a KeyType, &'a mut ValueType);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner_iterator
            .next()
            .map(|(key, value)| (&*key, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_release_create() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let index0 = pool.create_object(0, "item0".to_string());
        let _index1 = pool.create_object(1, "item1".to_string());
        let index2 = pool.create_object(2, "item2".to_string());
        let _index3 = pool.create_object(3, "item3".to_string());
        let index4 = pool.create_object(4, "item4".to_string());

        assert_eq!(
            pool.get_ref_by_index(index0),
            Some((&0, &"item0".to_string()))
        );
        assert_eq!(pool.get_ref_by_key(&1), Some((&1, &"item1".to_string())));
        assert_eq!(
            pool.get_ref_by_index(index2),
            Some((&2, &"item2".to_string()))
        );
        assert_eq!(pool.get_ref_by_key(&3), Some((&3, &"item3".to_string())));
        assert_eq!(
            pool.get_ref_by_index(index4),
            Some((&4, &"item4".to_string()))
        );

        assert_eq!(
            pool.release_object_by_index(index2),
            Some((2, "item2".to_string()))
        );
        assert_eq!(
            pool.release_object_by_key(&1),
            Some((1, "item1".to_string()))
        );
        assert_eq!(
            pool.release_object_by_index(index4),
            Some((4, "item4".to_string()))
        );

        let index5 = pool.create_object(5, "item5".to_string());
        assert_eq!(
            pool.get_ref_by_index(index5),
            Some((&5, &"item5".to_string()))
        );
    }

    #[test]
    fn accessing_released_object() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let _index0 = pool.create_object(0, "item0".to_string());
        let index1 = pool.create_object(1, "item1".to_string());
        let index2 = pool.create_object(2, "item2".to_string());
        let _index3 = pool.create_object(3, "item3".to_string());
        let _index4 = pool.create_object(4, "item4".to_string());

        assert_eq!(pool.len(), 5);

        assert_eq!(
            pool.release_object_by_key(&2),
            Some((2, "item2".to_string()))
        );
        assert_eq!(
            pool.release_object_by_index(index1),
            Some((1, "item1".to_string()))
        );
        assert_eq!(
            pool.release_object_by_key(&4),
            Some((4, "item4".to_string()))
        );

        assert_eq!(pool.len(), 2);

        assert_eq!(pool.get_ref_by_key(&1), None);
        assert_eq!(pool.get_ref_by_index(index2), None);
        assert_eq!(pool.get_ref_by_key(&4), None);

        assert_eq!(pool.get_mut_by_key(&1), None);
        assert_eq!(pool.get_mut_by_index(index2), None);
        assert_eq!(pool.get_mut_by_key(&4), None);
    }

    #[test]
    fn releasing_invalid_index() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let _index0 = pool.create_object(0, "item0".to_string());
        let _index1 = pool.create_object(1, "item1".to_string());
        let index2 = pool.create_object(2, "item2".to_string());
        let _index3 = pool.create_object(3, "item3".to_string());
        let _index4 = pool.create_object(4, "item4".to_string());

        assert_eq!(
            pool.release_object_by_index(index2),
            Some((2, "item2".to_string()))
        );
        assert!(pool.release_object_by_index(index2).is_none());
    }

    #[test]
    fn iterate_ref_on_empty() {
        let pool = ObjectMapPool::<isize, String>::new();
        let mut counter = 0;
        for _ in pool.iter() {
            counter += 1;
        }
        assert_eq!(counter, 0);
    }

    #[test]
    fn iterate_mut_on_empty() {
        let mut pool = ObjectMapPool::<isize, String>::new();
        let mut counter = 0;
        for _ in pool.iter_mut() {
            counter += 1;
        }
        assert_eq!(counter, 0);
    }

    #[test]
    fn iterate_ref() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let _index0 = pool.create_object(0, "item0".to_string());
        let index1 = pool.create_object(1, "item1".to_string());
        let _index2 = pool.create_object(2, "item2".to_string());
        let _index3 = pool.create_object(3, "item3".to_string());
        let index4 = pool.create_object(4, "item4".to_string());

        {
            let mut counter = 0;
            for item in pool.iter() {
                match counter {
                    0 => assert_eq!(item, (&0, &"item0".to_string())),
                    1 => assert_eq!(item, (&1, &"item1".to_string())),
                    2 => assert_eq!(item, (&2, &"item2".to_string())),
                    3 => assert_eq!(item, (&3, &"item3".to_string())),
                    4 => assert_eq!(item, (&4, &"item4".to_string())),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 5);
        }

        assert_eq!(
            pool.release_object_by_index(index1),
            Some((1, "item1".to_string()))
        );
        assert_eq!(
            pool.release_object_by_key(&3),
            Some((3, "item3".to_string()))
        );
        assert_eq!(
            pool.release_object_by_index(index4),
            Some((4, "item4".to_string()))
        );

        {
            let mut counter = 0;
            for item in pool.iter() {
                match counter {
                    0 => assert_eq!(item, (&0, &"item0".to_string())),
                    1 => assert_eq!(item, (&2, &"item2".to_string())),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 2);
        }
    }

    #[test]
    fn iterate_mut() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let index0 = pool.create_object(0, "item0".to_string());
        let _index1 = pool.create_object(1, "item1".to_string());
        let _index2 = pool.create_object(2, "item2".to_string());
        let index3 = pool.create_object(3, "item3".to_string());
        let _index4 = pool.create_object(4, "item4".to_string());

        {
            let mut counter = 0;
            for item in pool.iter_mut() {
                match counter {
                    0 => assert_eq!(item, (&0, &mut "item0".to_string())),
                    1 => assert_eq!(item, (&1, &mut "item1".to_string())),
                    2 => assert_eq!(item, (&2, &mut "item2".to_string())),
                    3 => assert_eq!(item, (&3, &mut "item3".to_string())),
                    4 => assert_eq!(item, (&4, &mut "item4".to_string())),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 5);
        }

        assert_eq!(
            pool.release_object_by_key(&1),
            Some((1, "item1".to_string()))
        );
        assert_eq!(
            pool.release_object_by_index(index3),
            Some((3, "item3".to_string()))
        );
        assert_eq!(
            pool.release_object_by_key(&4),
            Some((4, "item4".to_string()))
        );

        {
            let mut counter = 0;
            for item in pool.iter_mut() {
                match counter {
                    0 => assert_eq!(item, (&0, &mut "item0".to_string())),
                    1 => assert_eq!(item, (&2, &mut "item2".to_string())),
                    _ => (),
                };

                *item.1 = "new value".to_string();

                counter += 1;
            }
            assert_eq!(counter, 2);
        }

        assert_eq!(
            pool.get_ref_by_index(index0),
            Some((&0, &"new value".to_string()))
        );
        assert_eq!(
            pool.get_ref_by_key(&2),
            Some((&2, &"new value".to_string()))
        );
    }

    #[test]
    fn first_index() {
        let mut pool = ObjectMapPool::<isize, String>::new();

        let index0 = pool.create_object(0, "item0".to_string());
        let index1 = pool.create_object(1, "item1".to_string());
        let _index2 = pool.create_object(2, "item1".to_string());
        let index3 = pool.create_object(3, "item2".to_string());
        let _index4 = pool.create_object(4, "item2".to_string());
        let _index5 = pool.create_object(5, "item2".to_string());

        assert_eq!(
            pool.first_index(|_key, value| value == "item0"),
            Some(index0)
        );
        assert_eq!(
            pool.first_index(|_key, value| value == "item1"),
            Some(index1)
        );
        assert_eq!(
            pool.first_index(|_key, value| value == "item2"),
            Some(index3)
        );
        assert_eq!(pool.first_index(|_key, value| value == "item3"), None);
    }
}
