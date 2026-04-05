use alloc::{collections::VecDeque, vec::Vec};

#[macro_export]
macro_rules! define_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub struct $name(usize);

        impl From<usize> for $name {
            fn from(val: usize) -> Self {
                Self(val)
            }
        }

        impl From<$name> for usize {
            fn from(val: $name) -> Self {
                val.0
            }
        }
    };
}

pub struct IdSparseVec<I, T> {
    list: Vec<Option<T>>,
    free: VecDeque<I>,
}

impl<I, T> IdSparseVec<I, T> {
    pub const fn new() -> Self {
        IdSparseVec {
            list: Vec::new(),
            free: VecDeque::new(),
        }
    }
}

impl<I, T> IdSparseVec<I, T>
where
    I: Copy + TryFrom<usize> + Into<usize>,
{
    pub fn get(&self, id: I) -> Option<&T> {
        match self.list.get(id.into()) {
            Some(&Some(ref x)) => Some(x),
            _ => None,
        }
    }
    pub fn get_mut(&mut self, id: I) -> Option<&mut T> {
        match self.list.get_mut(id.into()) {
            Some(&mut Some(ref mut x)) => Some(x),
            _ => None,
        }
    }
    pub fn get_mut_2(&mut self, id1: I, id2: I) -> Option<(&mut T, &mut T)> {
        match self.list.get_disjoint_mut([id1.into(), id2.into()]) {
            Ok([&mut Some(ref mut x), &mut Some(ref mut y)]) => Some((x, y)),
            _ => None,
        }
    }
    pub fn insert(&mut self, value: T) -> I {
        if let Some(id) = self.free.pop_front() {
            self.list[id.into()] = Some(value);
            id
        } else {
            let Ok(id) = self.list.len().try_into() else {
                panic!("IdSparseVec overflow")
            };
            self.list.push(Some(value));
            id
        }
    }
    pub fn remove(&mut self, index: I) -> T {
        let old = self.list.get_mut(index.into()).unwrap().take();
        self.free.push_back(index);
        old.unwrap()
    }
}
