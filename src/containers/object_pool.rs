use std::cmp::Reverse;
use std::collections::BinaryHeap;

use or_die::OrDie;

pub trait ObjectPoolIndex:
    Sized + Into<DefaultObjectPoolIndex> + From<DefaultObjectPoolIndex>
{
    fn invalid() -> Self;
    fn invalidate(&mut self) -> Self;
    fn next_index<T>(&self, object_pool: &ObjectPool<T, Self>) -> Option<Self>;
}

impl<IndexType> ObjectPoolIndex for IndexType
where
    IndexType: From<DefaultObjectPoolIndex> + Into<DefaultObjectPoolIndex> + Clone,
{
    fn invalid() -> Self {
        DefaultObjectPoolIndex::invalid().into()
    }

    fn invalidate(&mut self) -> Self {
        let mut id = Self::invalid();
        std::mem::swap(&mut id, self);

        id
    }

    fn next_index<T>(&self, object_pool: &ObjectPool<T, Self>) -> Option<Self> {
        let default_index: DefaultObjectPoolIndex = self.clone().into();
        default_index
            .next_index(object_pool)
            .map(|index| index.into())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DefaultObjectPoolIndex {
    index: usize,
    version: isize,
}

impl DefaultObjectPoolIndex {
    pub fn invalid() -> Self {
        Self {
            index: 0,
            version: -1,
        }
    }

    pub fn invalidate(&mut self) -> Self {
        let mut id = Self::invalid();
        std::mem::swap(&mut id, self);

        id
    }

    pub fn next_index<T, IndexType: ObjectPoolIndex>(
        &self,
        object_pool: &ObjectPool<T, IndexType>,
    ) -> Option<Self> {
        let mut next_index = self.index + 1;
        while next_index < object_pool.objects.len() {
            if let Some(object_wrapper) = object_pool.objects.get(next_index)
                && object_wrapper.object.is_some()
            {
                return Some(DefaultObjectPoolIndex {
                    index: next_index,
                    version: object_wrapper.version,
                });
            }
            next_index += 1;
        }

        None
    }
}

struct ObjectWrapper<T> {
    version: isize,
    object: Option<T>,
}

pub struct ObjectPool<T, IndexType: ObjectPoolIndex> {
    objects: Vec<ObjectWrapper<T>>,
    free_slots: BinaryHeap<Reverse<usize>>,
    number_of_items: usize,
    _marker: std::marker::PhantomData<IndexType>,
}

impl<T, IndexType: ObjectPoolIndex> Default for ObjectPool<T, IndexType> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, IndexType: ObjectPoolIndex> ObjectPool<T, IndexType> {
    pub fn new() -> Self {
        ObjectPool {
            objects: Vec::new(),
            free_slots: BinaryHeap::new(),
            number_of_items: 0,
            _marker: Default::default(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        ObjectPool {
            objects: Vec::with_capacity(capacity),
            free_slots: BinaryHeap::with_capacity(capacity),
            number_of_items: 0,
            _marker: Default::default(),
        }
    }

    pub fn create_object(&mut self, value: T) -> IndexType {
        match self.free_slots.pop() {
            Some(Reverse(index)) => {
                let obj = &mut self.objects[index];
                obj.object = Some(value);
                obj.version += 1;

                self.number_of_items += 1;

                DefaultObjectPoolIndex {
                    index,
                    version: obj.version,
                }
                .into()
            }
            None => {
                let index = self.objects.len();
                let version = 1;

                self.objects.push(ObjectWrapper {
                    version,
                    object: Some(value),
                });

                self.number_of_items += 1;

                DefaultObjectPoolIndex { index, version }.into()
            }
        }
    }

    pub fn create_object_with_fn<ErrorType>(
        &mut self,
        f: impl FnOnce(&IndexType) -> Result<T, ErrorType>,
    ) -> Result<(IndexType, &T), ErrorType> {
        // pop should be issued after the call to f because it may panic!
        let (index, pool_index) = match self.free_slots.peek() {
            Some(Reverse(index)) => {
                let index = *index;
                let obj = &mut self.objects[index];

                let pool_index: IndexType = DefaultObjectPoolIndex {
                    index,
                    version: obj.version + 1,
                }
                .into();

                // call to f may panic! therefore using pop
                obj.object = Some(f(&pool_index)?);
                obj.version += 1;
                self.free_slots.pop();
                self.number_of_items += 1;

                (index, pool_index)
            }
            None => {
                let index = self.objects.len();
                let version = 1;

                let pool_index: IndexType = DefaultObjectPoolIndex { index, version }.into();

                self.objects.push(ObjectWrapper {
                    version,
                    object: Some(f(&pool_index)?),
                });

                self.number_of_items += 1;

                (index, pool_index)
            }
        };

        Ok((pool_index, self.objects[index].object.as_ref().or_die()))
    }

    pub fn release_object(&mut self, index: IndexType) -> Option<T> {
        let index: DefaultObjectPoolIndex = index.into();
        if index.index < self.objects.len() {
            let obj = &mut self.objects[index.index];
            if obj.version == index.version {
                obj.version += 1;
                self.free_slots.push(Reverse(index.index));

                self.number_of_items -= 1;

                let mut object_opt = None;
                std::mem::swap(&mut object_opt, &mut obj.object);

                object_opt
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn get_ref(&self, index: IndexType) -> Option<&T> {
        let index: DefaultObjectPoolIndex = index.into();
        if index.index < self.objects.len() {
            let obj = &self.objects[index.index];
            if obj.version == index.version {
                return obj.object.as_ref();
            }
        }

        None
    }

    pub fn get_mut(&mut self, index: IndexType) -> Option<&mut T> {
        let index: DefaultObjectPoolIndex = index.into();
        if index.index < self.objects.len() {
            let obj = &mut self.objects[index.index];
            if obj.version == index.version {
                return obj.object.as_mut();
            }
        }

        None
    }

    pub fn iter(&self) -> ObjectPoolIter<'_, T> {
        ObjectPoolIter {
            inner_iterator: self.objects.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> ObjectPoolIterMut<'_, T> {
        ObjectPoolIterMut {
            inner_iterator: self.objects.iter_mut(),
        }
    }

    pub fn len(&self) -> usize {
        self.number_of_items
    }

    pub fn is_empty(&self) -> bool {
        self.number_of_items == 0
    }

    pub fn find_first_index(&self, pred: impl Fn(&T) -> bool) -> Option<IndexType> {
        self.objects
            .iter()
            .position(|object_wrapper| {
                if let Some(object) = object_wrapper.object.as_ref() {
                    pred(object)
                } else {
                    false
                }
            })
            .map(|index| {
                DefaultObjectPoolIndex {
                    index,
                    version: self.objects[index].version,
                }
                .into()
            })
    }

    pub fn first_index(&self) -> Option<IndexType> {
        self.objects
            .iter()
            .position(|object_wrapper| object_wrapper.object.is_some())
            .map(|index| {
                DefaultObjectPoolIndex {
                    index,
                    version: self.objects[index].version,
                }
                .into()
            })
    }
}

pub struct ObjectPoolIter<'a, T> {
    inner_iterator: std::slice::Iter<'a, ObjectWrapper<T>>,
}

impl<'a, T> Iterator for ObjectPoolIter<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        for object_wrapper in self.inner_iterator.by_ref() {
            let object = object_wrapper.object.as_ref();
            if object.is_some() {
                return object;
            } else {
                continue;
            }
        }

        None
    }
}

pub struct ObjectPoolIterMut<'a, T> {
    inner_iterator: std::slice::IterMut<'a, ObjectWrapper<T>>,
}

impl<'a, T> Iterator for ObjectPoolIterMut<'a, T> {
    type Item = &'a mut T;

    fn next(&mut self) -> Option<Self::Item> {
        for object_wrapper in self.inner_iterator.by_ref() {
            let object = object_wrapper.object.as_mut();
            if object.is_some() {
                return object;
            } else {
                continue;
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use crate::infallible::UnwrapInfallible;

    use super::*;

    #[test]
    fn create_release_create() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let index3 = pool
            .create_object_with_fn(|_index| Ok("item3".to_string()))
            .infallible()
            .0;
        let index4 = pool
            .create_object_with_fn(|_index| Ok("item4".to_string()))
            .infallible()
            .0;

        assert_eq!(
            index0,
            DefaultObjectPoolIndex {
                index: 0,
                version: 1
            }
        );
        assert_eq!(
            index1,
            DefaultObjectPoolIndex {
                index: 1,
                version: 1
            }
        );
        assert_eq!(
            index2,
            DefaultObjectPoolIndex {
                index: 2,
                version: 1
            }
        );
        assert_eq!(
            index3,
            DefaultObjectPoolIndex {
                index: 3,
                version: 1
            }
        );
        assert_eq!(
            index4,
            DefaultObjectPoolIndex {
                index: 4,
                version: 1
            }
        );

        assert_eq!(pool.get_ref(index0).cloned(), Some("item0".to_string()));
        assert_eq!(pool.get_ref(index1).cloned(), Some("item1".to_string()));
        assert_eq!(pool.get_ref(index2).cloned(), Some("item2".to_string()));
        assert_eq!(pool.get_ref(index3).cloned(), Some("item3".to_string()));
        assert_eq!(pool.get_ref(index4).cloned(), Some("item4".to_string()));

        assert_eq!(pool.release_object(index2), Some("item2".to_string()));
        assert_eq!(pool.release_object(index1), Some("item1".to_string()));
        assert_eq!(pool.release_object(index4), Some("item4".to_string()));

        let index5 = pool.create_object("item5".to_string());
        assert_eq!(
            index5,
            DefaultObjectPoolIndex {
                index: 1,
                version: 3
            }
        );
        assert_eq!(pool.get_ref(index5).cloned(), Some("item5".to_string()));
    }

    #[test]
    fn accessing_released_object() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let _index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let _index3 = pool
            .create_object_with_fn(|_index| Ok("item3".to_string()))
            .infallible()
            .0;
        let index4 = pool
            .create_object_with_fn(|_index| Ok("item4".to_string()))
            .infallible()
            .0;

        assert_eq!(pool.len(), 5);

        assert_eq!(pool.release_object(index2), Some("item2".to_string()));
        assert_eq!(pool.release_object(index1), Some("item1".to_string()));
        assert_eq!(pool.release_object(index4), Some("item4".to_string()));

        assert_eq!(pool.len(), 2);

        assert_eq!(pool.get_ref(index1), None);
        assert_eq!(pool.get_ref(index2), None);
        assert_eq!(pool.get_ref(index4), None);

        assert_eq!(pool.get_mut(index1), None);
        assert_eq!(pool.get_mut(index2), None);
        assert_eq!(pool.get_mut(index4), None);
    }

    #[test]
    fn releasing_invalid_index() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let _index0 = pool.create_object("item0".to_string());
        let _index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let _index3 = pool
            .create_object_with_fn(|_index| Ok("item3".to_string()))
            .infallible()
            .0;
        let _index4 = pool
            .create_object_with_fn(|_index| Ok("item4".to_string()))
            .infallible()
            .0;

        assert_eq!(pool.release_object(index2), Some("item2".to_string()));
        assert!(pool.release_object(index2).is_none());
    }

    #[test]
    fn iterate_ref_on_empty() {
        let pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();
        let mut counter = 0;
        for _ in pool.iter() {
            counter += 1;
        }
        assert_eq!(counter, 0);
    }

    #[test]
    fn iterate_mut_on_empty() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();
        let mut counter = 0;
        for _ in pool.iter_mut() {
            counter += 1;
        }
        assert_eq!(counter, 0);
    }

    #[test]
    fn iterate_ref() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let _index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let _index2 = pool.create_object("item2".to_string());
        let index3 = pool
            .create_object_with_fn(|_index| Ok("item3".to_string()))
            .infallible()
            .0;
        let index4 = pool
            .create_object_with_fn(|_index| Ok("item4".to_string()))
            .infallible()
            .0;

        {
            let mut counter = 0;
            for item in pool.iter() {
                match counter {
                    0 => assert_eq!(item, "item0"),
                    1 => assert_eq!(item, "item1"),
                    2 => assert_eq!(item, "item2"),
                    3 => assert_eq!(item, "item3"),
                    4 => assert_eq!(item, "item4"),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 5);
        }

        assert_eq!(pool.release_object(index1), Some("item1".to_string()));
        assert_eq!(pool.release_object(index3), Some("item3".to_string()));
        assert_eq!(pool.release_object(index4), Some("item4".to_string()));

        {
            let mut counter = 0;
            for item in pool.iter() {
                match counter {
                    0 => assert_eq!(item, "item0"),
                    1 => assert_eq!(item, "item2"),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 2);
        }
    }

    #[test]
    fn iterate_mut() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let index3 = pool
            .create_object_with_fn(|_index| Ok("item3".to_string()))
            .infallible()
            .0;
        let index4 = pool
            .create_object_with_fn(|_index| Ok("item4".to_string()))
            .infallible()
            .0;

        {
            let mut counter = 0;
            for item in pool.iter_mut() {
                match counter {
                    0 => assert_eq!(item, "item0"),
                    1 => assert_eq!(item, "item1"),
                    2 => assert_eq!(item, "item2"),
                    3 => assert_eq!(item, "item3"),
                    4 => assert_eq!(item, "item4"),
                    _ => (),
                };

                counter += 1;
            }
            assert_eq!(counter, 5);
        }

        assert_eq!(pool.release_object(index1), Some("item1".to_string()));
        assert_eq!(pool.release_object(index3), Some("item3".to_string()));
        assert_eq!(pool.release_object(index4), Some("item4".to_string()));

        {
            let mut counter = 0;
            for item in pool.iter_mut() {
                match counter {
                    0 => assert_eq!(item, "item0"),
                    1 => assert_eq!(item, "item2"),
                    _ => (),
                };

                *item = "new value".to_string();

                counter += 1;
            }
            assert_eq!(counter, 2);
        }

        assert_eq!(pool.get_ref(index0).cloned(), Some("new value".to_string()));
        assert_eq!(pool.get_ref(index2).cloned(), Some("new value".to_string()));
    }

    #[test]
    fn first_index() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let _index2 = pool.create_object("item1".to_string());
        let index3 = pool.create_object("item2".to_string());
        let _index4 = pool.create_object("item2".to_string());
        let _index5 = pool.create_object("item2".to_string());

        assert_eq!(pool.find_first_index(|item| item == "item0"), Some(index0));
        assert_eq!(pool.find_first_index(|item| item == "item1"), Some(index1));
        assert_eq!(pool.find_first_index(|item| item == "item2"), Some(index3));
        assert_eq!(pool.find_first_index(|item| item == "item3"), None);
    }

    #[test]
    fn next_index_on_empty_pool() {
        let pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        assert_eq!(pool.first_index(), None);
    }

    #[test]
    fn next_index_finds_immediate_next() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());

        let first = pool.first_index().unwrap();

        assert_eq!(first, index0);
        assert_eq!(first.next_index(&pool), Some(index1));
        assert_eq!(index1.next_index(&pool), None);
    }

    #[test]
    fn next_index_skips_released_slots() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let _index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let index3 = pool.create_object("item3".to_string());
        let first = pool.first_index().unwrap();

        assert_eq!(pool.release_object(index1), Some("item1".to_string()));
        assert_eq!(pool.release_object(index2), Some("item2".to_string()));

        assert_eq!(first.next_index(&pool), Some(index3));
        assert_eq!(index3.next_index(&pool), None);
    }

    #[test]
    fn next_index_from_stale_handle_uses_position_not_version() {
        let mut pool = ObjectPool::<String, DefaultObjectPoolIndex>::new();

        let index0 = pool.create_object("item0".to_string());
        let index1 = pool.create_object("item1".to_string());
        let index2 = pool.create_object("item2".to_string());
        let first = pool.first_index().unwrap();

        assert_eq!(pool.release_object(index0), Some("item0".to_string()));

        // `next` advances by index position and does not validate the current handle version.
        assert_eq!(first.next_index(&pool), Some(index1));
        assert_eq!(index1.next_index(&pool), Some(index2));
    }
}
