use crate::{
    memory::{MemoryError, direct_alloc_ptr},
    prelude::*,
};

use core::{cmp::Ordering, marker::PhantomData, ptr::null_mut};

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Color {
    Red,
    Black,
}
use Color::*;

#[derive(PartialEq, Eq, Debug, Clone, Copy)]
enum Direction {
    Left,
    Right,
}
use Direction::*;
use alloc::boxed::Box;

impl core::ops::Not for Direction {
    type Output = Direction;

    fn not(self) -> Self::Output {
        match self {
            Left => Right,
            Right => Left,
        }
    }
}

pub trait Augment<T>
where
    Self: Sized,
{
    fn augment(node: &T, left: &Option<Self>, right: &Option<Self>) -> Self;
}

impl<T> Augment<T> for () {
    fn augment(_node: &T, _left: &Option<Self>, _right: &Option<Self>) -> Self {
        ()
    }
}

pub struct RbNode<T, V> {
    color: Color,
    parent: *mut RbNode<T, V>,
    left: *mut RbNode<T, V>,
    right: *mut RbNode<T, V>,
    value: T,
    augment: Option<V>,
}

fn mark_dirty<T, V>(mut node: *mut RbNode<T, V>) {
    unsafe {
        while !node.is_null() && !(*node).augment.is_none() {
            (*node).augment.take();
            node = (*node).parent;
        }
    }
}

impl<T, V> RbNode<T, V> {
    pub fn new(value: T) -> Self {
        RbNode {
            color: Red,
            parent: null_mut(),
            left: null_mut(),
            right: null_mut(),
            value,
            augment: None,
        }
    }
    pub fn left(&self) -> Option<&RbNode<T, V>> {
        unsafe { self.left.as_ref() }
    }
    pub fn right(&self) -> Option<&RbNode<T, V>> {
        unsafe { self.right.as_ref() }
    }
    pub fn parent(&self) -> Option<&RbNode<T, V>> {
        unsafe { self.parent.as_ref() }
    }
    pub fn value(&self) -> &T {
        &self.value
    }
    // SAFETY: must not do changes that affect ordering or augment values
    pub unsafe fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }
    pub fn into_value(self) -> T {
        self.value
    }
    pub fn augment(&self) -> &V {
        self.augment
            .as_ref()
            .expect("no augment value -- can't happen")
    }
    fn child(&self, dir: Direction) -> *mut RbNode<T, V> {
        match dir {
            Left => self.left,
            Right => self.right,
        }
    }

    fn set_child(&mut self, dir: Direction, value: *mut RbNode<T, V>) {
        match dir {
            Left => self.left = value,
            Right => self.right = value,
        }
        unsafe {
            mark_dirty(self);
            if !value.is_null() {
                (*value).parent = self;
            }
        }
    }
    fn child_dir(&self, child: *mut RbNode<T, V>) -> Direction {
        assert!(self.left == child || self.right == child);
        if self.left == child { Left } else { Right }
    }
    fn grandparent(&self) -> *mut RbNode<T, V> {
        unsafe {
            if !self.parent.is_null() {
                (*self.parent).parent
            } else {
                null_mut()
            }
        }
    }
    fn uncle(&self) -> *mut RbNode<T, V> {
        unsafe {
            let grandparent = self.grandparent();
            if !grandparent.is_null() {
                if (*grandparent).left == self.parent {
                    (*grandparent).right
                } else {
                    (*grandparent).left
                }
            } else {
                null_mut()
            }
        }
    }
}

pub struct RbNodeRef<'a, T, V> {
    tree: &'a RbTree<T, V>,
    node: *const RbNode<T, V>,
}

pub struct RbNodeMut<'a, T, V> {
    tree: &'a mut RbTree<T, V>,
    node: *mut RbNode<T, V>,
}

pub struct OwnedRbNode<T, V> {
    node: *mut RbNode<T, V>,
}

impl<T, V> OwnedRbNode<T, V> {
    pub fn value(&self) -> &T {
        unsafe { &(*self.node).value }
    }
    pub fn value_mut(&mut self) -> &mut T {
        unsafe { &mut (*self.node).value }
    }
    pub fn replace_value(&mut self, new_value: T) -> T {
        core::mem::replace(self.value_mut(), new_value)
    }
}

pub struct RbTree<T, V> {
    head: *mut RbNode<T, V>,
}

unsafe impl<T: Send, V: Send> Send for RbTree<T, V> {}
unsafe impl<T: Sync, V: Sync> Sync for RbTree<T, V> {}
unsafe impl<T: Send, V: Send> Send for RbNodeRef<'_, T, V> {}
unsafe impl<T: Sync, V: Sync> Sync for RbNodeRef<'_, T, V> {}
unsafe impl<T: Send, V: Send> Send for RbNodeMut<'_, T, V> {}
unsafe impl<T: Sync, V: Sync> Sync for RbNodeMut<'_, T, V> {}
unsafe impl<T: Send, V: Send> Send for OwnedRbNode<T, V> {}
unsafe impl<T: Sync, V: Sync> Sync for OwnedRbNode<T, V> {}

fn color<T, V>(node: *mut RbNode<T, V>) -> Color {
    unsafe { if node.is_null() { Black } else { (*node).color } }
}

fn is_dirty<T, V>(node: *mut RbNode<T, V>) -> bool {
    unsafe {
        if node.is_null() {
            false
        } else {
            (*node).augment.is_none()
        }
    }
}

fn place<T, V>(
    head: *mut *mut RbNode<T, V>,
    node: *mut RbNode<T, V>,
    cmp: impl Fn(&T, &T) -> Ordering,
) {
    unsafe {
        (*node).color = Red;
        (*node).left = null_mut();
        (*node).right = null_mut();
        (*node).augment = None;

        let mut parent = null_mut();
        let mut link = head;
        while !(*link).is_null() {
            parent = *link;
            if cmp(&(*node).value, &(*parent).value).is_le() {
                link = &raw mut (*parent).left;
            } else {
                link = &raw mut (*parent).right;
            }
        }
        (*node).parent = parent;
        *link = node;
    }
}

fn successor_child<T, V>(node: *mut RbNode<T, V>) -> *mut RbNode<T, V> {
    unsafe {
        let mut n = (*node).right;
        while !(*n).left.is_null() {
            n = (*n).left;
        }
        n
    }
}

impl<T, V> RbTree<T, V> {
    pub const fn new() -> Self {
        RbTree { head: null_mut() }
    }
    pub fn head(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self,
            node: unsafe { self.head.as_ref()? },
        })
    }
    pub fn is_empty(&self) -> bool {
        self.head.is_null()
    }
}

impl<T, V> RbTree<T, V>
where
    V: Augment<T>,
{
    fn recolor(&mut self, mut node: *mut RbNode<T, V>) -> *mut RbNode<T, V> {
        // we know node is red
        // parent might also be red, which we try to fix by just recoloring nodes
        // red invariant holds for the rest of the tree though
        // and black invariant holds since we added only a red node so far
        unsafe {
            loop {
                let parent = (*node).parent;
                let grandparent = (*node).grandparent();
                let uncle = (*node).uncle();
                if color(parent) == Red && color(uncle) == Red {
                    // if we land here then we know for sure:
                    // 1. parent, grandparent and uncle all exist (implied by color(uncle)==Red)
                    // 2. grandparent is black (red-invariant previously held)
                    // 3. red invariant is now broken bc self and parent are both red
                    //
                    // try to fix the red invariant by flipping all three
                    // this preserves the black invariant but we might have broken red invariant for grandparent
                    (*grandparent).color = Red;
                    (*parent).color = Black;
                    (*uncle).color = Black;
                    node = grandparent;
                } else {
                    break;
                }
            }
        }
        // node is red, either parent or uncle is black
        node
    }
    fn calculate_augment(&mut self, node: *mut RbNode<T, V>) {
        unsafe {
            let left = if (*node).left.is_null() {
                &None
            } else {
                &(*(*node).left).augment
            };
            let right = if (*node).right.is_null() {
                &None
            } else {
                &(*(*node).right).augment
            };
            (*node).augment = Some(V::augment(&(*node).value, left, right));
        }
    }
    fn update_augments(&mut self) {
        unsafe {
            let mut node = self.head;
            if !is_dirty(node) {
                return;
            }
            loop {
                if is_dirty((*node).left) {
                    node = (*node).left;
                } else if is_dirty((*node).right) {
                    node = (*node).right;
                } else {
                    self.calculate_augment(node);
                    node = (*node).parent;
                    if node.is_null() {
                        return;
                    }
                }
            }
        }
    }
    fn replace_node(&mut self, node: *mut RbNode<T, V>, replacement: *mut RbNode<T, V>) {
        unsafe {
            let parent = (*node).parent;
            if parent.is_null() {
                self.head = replacement;
                if !replacement.is_null() {
                    (*replacement).parent = null_mut();
                }
            } else {
                (*parent).set_child((*parent).child_dir(node), replacement);
            }
        }
    }
    fn rotate(&mut self, node: *mut RbNode<T, V>) {
        // we know that node is red, parent is red, grandparent is black
        unsafe {
            let mut parent = (*node).parent;
            let grandparent = (*node).grandparent();
            if grandparent.is_null() {
                (*parent).color = Black;
                return;
            }
            // first we want to make sure that parent-dir and node-dir are the same, if they're not swap some nodes around to make this true
            let side = (*grandparent).child_dir(parent);
            if side != (*parent).child_dir(node) {
                let child_outer = (*node).child(side);
                (*grandparent).set_child(side, node);
                (*node).set_child(side, parent);
                (*parent).set_child(!side, child_outer);
                parent = node;
                // node isn't used below
            }
            // now we want parent to become root of the subtree, with the two children node and grandparent
            let sibling = (*parent).child(!side);
            self.replace_node(grandparent, parent);
            (*parent).set_child(!side, grandparent);
            (*grandparent).set_child(side, sibling);
            // finally swap colors of parent and grandparent
            (*parent).color = Black;
            (*grandparent).color = Red;
        }
    }
    pub fn find_raw(&self, mut eval: impl FnMut(&T, &V) -> Ordering) -> Option<*mut RbNode<T, V>> {
        unsafe {
            let mut node = self.head;
            while !node.is_null() {
                match eval(&(*node).value, (*node).augment()) {
                    Ordering::Greater => node = (*node).left,
                    Ordering::Equal => return Some(node),
                    Ordering::Less => node = (*node).right,
                }
            }
            None
        }
    }
    pub fn find(&self, eval: impl FnMut(&T, &V) -> Ordering) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self,
            node: self.find_raw(eval)?,
        })
    }
    pub fn find_mut(
        &mut self,
        eval: impl FnMut(&T, &V) -> Ordering,
    ) -> Option<RbNodeMut<'_, T, V>> {
        let node = self.find_raw(eval)?;
        Some(RbNodeMut { tree: self, node })
    }
    pub fn lower_bound_raw(
        &self,
        mut eval: impl FnMut(&T, &V) -> bool,
    ) -> Option<*mut RbNode<T, V>> {
        unsafe {
            let mut node = self.head;
            let mut candidate = None;
            while !node.is_null() {
                if eval(&(*node).value, (*node).augment()) {
                    candidate = Some(node);
                    node = (*node).left;
                } else {
                    node = (*node).right;
                }
            }
            candidate
        }
    }
    pub fn lower_bound(&self, eval: impl FnMut(&T, &V) -> bool) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self,
            node: self.lower_bound_raw(eval)?,
        })
    }
    pub fn lower_bound_mut(
        &mut self,
        eval: impl FnMut(&T, &V) -> bool,
    ) -> Option<RbNodeMut<'_, T, V>> {
        let node = self.lower_bound_raw(eval)?;
        Some(RbNodeMut { tree: self, node })
    }
    // SAFETY: node is a valid pointer. you are passing ownership to the tree
    // note that the previous value in node.augment is dropped, be careful with pointer allocation here
    pub unsafe fn insert_raw(&mut self, node: *mut RbNode<T, V>, cmp: impl Fn(&T, &T) -> Ordering) {
        unsafe {
            place(&raw mut self.head, node, cmp);
            mark_dirty((*node).parent);
            let w_node = self.recolor(node);
            if color((*w_node).parent) == Red {
                self.rotate(w_node);
            }
            self.update_augments();
        }
    }
    pub fn insert(&mut self, node: OwnedRbNode<T, V>, cmp: impl Fn(&T, &T) -> Ordering) {
        unsafe { self.insert_raw(node.into_raw(), cmp) };
    }
    // removes the node, retaining the correct order
    // returns (parent, child, removed_color) for the location where we broke red-black invariants
    // parent and child may both be null
    fn unplace(
        &mut self,
        node: *mut RbNode<T, V>,
    ) -> (*mut RbNode<T, V>, *mut RbNode<T, V>, Color) {
        unsafe {
            match ((*node).left.is_null(), (*node).right.is_null()) {
                (true, _) => {
                    self.replace_node(node, (*node).right);
                    ((*node).parent, (*node).right, (*node).color)
                }
                (false, true) => {
                    self.replace_node(node, (*node).left);
                    ((*node).parent, (*node).left, (*node).color)
                }
                (false, false) => {
                    // we have to splice the successor into the original location
                    let succ = successor_child(node);
                    // successor has no left node so we can replace it by its child
                    let fixup_parent = if (*succ).parent == node {
                        succ
                    } else {
                        (*succ).parent
                    };
                    let replacement = (*succ).right;
                    self.replace_node(succ, (*succ).right);
                    self.replace_node(node, succ);
                    (*succ).set_child(Left, (*node).left);
                    (*succ).set_child(Right, (*node).right);
                    core::mem::swap(&mut (*node).color, &mut (*succ).color);
                    (fixup_parent, replacement, (*node).color)
                }
            }
        }
    }
    fn remove_fixup(&mut self, mut parent: *mut RbNode<T, V>, mut deficit_side: Direction) {
        unsafe {
            loop {
                let mut sibling = (*parent).child(!deficit_side);
                // if the sibling is red, we can rotate to the case with a black sibling
                if color(sibling) == Red {
                    let sibling_child = (*sibling).child(deficit_side);
                    self.replace_node(parent, sibling);
                    (*sibling).set_child(deficit_side, parent);
                    (*parent).set_child(!deficit_side, sibling_child);
                    core::mem::swap(&mut (*sibling).color, &mut (*parent).color);
                    sibling = sibling_child;
                }
                // sibling must be black now
                if color((*sibling).left) == Black && color((*sibling).right) == Black {
                    // no black children, so we can just paint the sibling red
                    (*sibling).color = Red;
                    // both sides of parent are now balanced, but the two trees below grandparent are imbalanced
                    if color(parent) == Red {
                        // if parent is red, just make it black, and everything is balanced
                        (*parent).color = Black;
                        return;
                    }
                    if (*parent).parent.is_null() {
                        return;
                    }
                    // iterate going up
                    deficit_side = (*(*parent).parent).child_dir(parent);
                    parent = (*parent).parent;
                    continue;
                }
                let inner_child = (*sibling).child(deficit_side);
                let outer_child = (*sibling).child(!deficit_side);
                if color(outer_child) == Black {
                    // inner_child must be red, rotate to make the new inner child black
                    let grand_child = (*inner_child).child(!deficit_side);
                    (*parent).set_child(!deficit_side, inner_child);
                    (*inner_child).set_child(!deficit_side, sibling);
                    (*sibling).set_child(deficit_side, grand_child);
                    core::mem::swap(&mut (*sibling).color, &mut (*inner_child).color);
                    sibling = inner_child;
                }
                let inner_child = (*sibling).child(deficit_side);
                let outer_child = (*sibling).child(!deficit_side);
                // finally rotate sibling to the top, then re-assign colors to balance tree
                self.replace_node(parent, sibling);
                (*sibling).set_child(deficit_side, parent);
                (*parent).set_child(!deficit_side, inner_child);
                (*sibling).color = (*parent).color;
                (*parent).color = Black;
                (*outer_child).color = Black;
                return;
            }
        }
    }
    // SAFETY: node is a valid pointer to a node currently in the tree
    // you might need to drop and free the memory it points to afterwards
    pub unsafe fn remove_raw(&mut self, node: *mut RbNode<T, V>) {
        unsafe {
            let (fixup_parent, fixup_child, removed_color) = self.unplace(node);
            if removed_color == Red {
                // removing a red node changes nothing about the invariants
            } else if !fixup_child.is_null() {
                // if there is a child in the new position, it must be red and we can just change it to black
                debug_assert_eq!((*fixup_child).color, Red);
                (*fixup_child).color = Black;
            } else if fixup_parent.is_null() {
                // tree is now empty
            } else {
                let deficit_side = if (*fixup_parent).left.is_null() {
                    Left
                } else {
                    Right
                };
                self.remove_fixup(fixup_parent, deficit_side);
            }
            self.update_augments();
        }
    }
}

impl<'a, T, V> RbNodeRef<'a, T, V> {
    pub fn value(&self) -> &'a T {
        unsafe { &(*self.node).value }
    }
    pub fn augment(&self) -> &'a V {
        unsafe { (*self.node).augment() }
    }
    pub fn left(&self) -> Option<RbNodeRef<'a, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).left.as_ref()? },
        })
    }
    pub fn right(&self) -> Option<RbNodeRef<'a, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).right.as_ref()? },
        })
    }
    pub fn parent(&self) -> Option<RbNodeRef<'a, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).parent.as_ref()? },
        })
    }
    pub fn successor(&self) -> Option<RbNodeRef<'a, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).successor()? },
        })
    }
}

impl<'a, T, V> RbNodeMut<'a, T, V> {
    pub fn value(&self) -> &'a T {
        unsafe { &(*self.node).value }
    }
    pub fn augment(&self) -> &'a V {
        unsafe { (*self.node).augment() }
    }
    pub fn left(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).left.as_ref()? },
        })
    }
    pub fn right(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).right.as_ref()? },
        })
    }
    pub fn parent(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).parent.as_ref()? },
        })
    }
    pub fn successor(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self.tree,
            node: unsafe { (*self.node).successor()? },
        })
    }
    // SAFETY: any changes you make must not invalidate the ordering or augment values
    pub unsafe fn value_mut(&mut self) -> &'a mut T {
        unsafe { &mut (*self.node).value }
    }
    pub fn left_mut(&mut self) -> Option<RbNodeMut<'_, T, V>> {
        Some(RbNodeMut {
            tree: self.tree,
            node: unsafe { (*self.node).left.as_mut()? },
        })
    }
    pub fn right_mut(&mut self) -> Option<RbNodeMut<'_, T, V>> {
        Some(RbNodeMut {
            tree: self.tree,
            node: unsafe { (*self.node).right.as_mut()? },
        })
    }
    pub fn parent_mut(&mut self) -> Option<RbNodeMut<'_, T, V>> {
        Some(RbNodeMut {
            tree: self.tree,
            node: unsafe { (*self.node).parent.as_mut()? },
        })
    }
    pub fn successor_mut(&mut self) -> Option<RbNodeMut<'_, T, V>> {
        Some(RbNodeMut {
            tree: self.tree,
            node: unsafe { (*self.node).successor()? },
        })
    }
}

impl<T, V: Augment<T>> RbNodeMut<'_, T, V> {
    pub fn insert(&mut self, node: OwnedRbNode<T, V>, cmp: impl Fn(&T, &T) -> Ordering) {
        self.tree.insert(node, cmp);
    }
    pub fn remove(self) -> OwnedRbNode<T, V> {
        let RbNodeMut { tree, node } = self;
        unsafe { tree.remove_raw(node) };
        OwnedRbNode { node }
    }
}

impl<T, V> OwnedRbNode<T, V> {
    pub fn new(value: T) -> OwnedRbNode<T, V> {
        OwnedRbNode {
            node: Box::leak(Box::new(RbNode::new(value))),
        }
    }
    // note this is safe to drop bc kernel_free handles direct allocations just fine
    pub fn new_direct(value: T) -> Result<OwnedRbNode<T, V>, MemoryError> {
        let node = direct_alloc_ptr()?;
        unsafe { core::ptr::write(node, RbNode::new(value)) };
        Ok(OwnedRbNode { node })
    }
    pub fn into_raw(self) -> *mut RbNode<T, V> {
        let node = self.node;
        core::mem::forget(self);
        node
    }
    pub fn into_value(self) -> T {
        unsafe { Box::from_raw(self.into_raw()) }.value
    }
}

impl<T, V> Drop for OwnedRbNode<T, V> {
    fn drop(&mut self) {
        unsafe { drop(Box::from_raw(self.node)) };
    }
}

impl<T, V: Augment<T> + Eq + core::fmt::Debug> RbTree<T, V> {
    pub fn check(&self, cmp: impl Fn(&T, &T) -> Ordering) {
        crate::memory::rbtree::tests::tree_check(self.head, null_mut(), None, None, &cmp);
    }
}

impl<T, V> RbTree<T, V> {
    pub fn lowest_node_raw(&self) -> Option<*mut RbNode<T, V>> {
        let mut node = self.head;
        unsafe {
            if !node.is_null() {
                while !(*node).left.is_null() {
                    node = (*node).left;
                }
                Some(node)
            } else {
                None
            }
        }
    }
    pub fn lowest_node(&self) -> Option<RbNodeRef<'_, T, V>> {
        Some(RbNodeRef {
            tree: self,
            node: self.lowest_node_raw()?,
        })
    }
    pub fn lowest_node_mut(&mut self) -> Option<RbNodeMut<'_, T, V>> {
        let node = self.lowest_node_raw()?;
        Some(RbNodeMut { tree: self, node })
    }
    pub fn iter(&self) -> RbTreeIterator<'_, T, V> {
        RbTreeIterator {
            node: self.lowest_node_raw().unwrap_or_default(),
            _phantom: PhantomData::default(),
        }
    }
}

impl<T, V> RbNode<T, V> {
    pub fn successor(&self) -> Option<*mut RbNode<T, V>> {
        let mut n = self as *const RbNode<T, V> as *mut RbNode<T, V>;
        unsafe {
            if (*n).right.is_null() {
                loop {
                    let child = n;
                    n = (*n).parent;
                    if n.is_null() {
                        return None;
                    }
                    if (*n).left == child {
                        return Some(n);
                    }
                }
            } else {
                return Some(successor_child(n));
            }
        }
    }
}

pub struct RbTreeIterator<'a, T, V> {
    node: *mut RbNode<T, V>,
    _phantom: PhantomData<&'a RbNode<T, V>>,
}

impl<'a, T, V> Iterator for RbTreeIterator<'a, T, V> {
    type Item = &'a RbNode<T, V>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.node.is_null() {
            None
        } else {
            unsafe {
                let ret = self.node;
                self.node = (*self.node).successor().unwrap_or_default();
                Some(&*ret)
            }
        }
    }
}

impl<T: Clone, V: Clone> RbNode<T, V> {
    fn clone_node(&self) -> OwnedRbNode<T, V> {
        let new_node = OwnedRbNode::new(self.value.clone());
        let clone_child = |n: &RbNode<T, V>| {
            let m = n.clone_node();
            unsafe { (*m.node).parent = new_node.node };
            m
        };
        let left = unsafe { self.left.as_ref().map(clone_child) };
        let right = unsafe { self.right.as_ref().map(clone_child) };
        unsafe {
            (*new_node.node).color = self.color;
            (*new_node.node).left = left.map(|n| n.into_raw()).unwrap_or_default();
            (*new_node.node).right = right.map(|n| n.into_raw()).unwrap_or_default();
            (*new_node.node).augment = self.augment.clone();
        }
        new_node
    }
}

impl<T: Clone, V: Clone> Clone for RbTree<T, V> {
    fn clone(&self) -> Self {
        unsafe {
            RbTree {
                head: self
                    .head
                    .as_ref()
                    .map(|n| n.clone_node().into_raw())
                    .unwrap_or_default(),
            }
        }
    }
}

impl<T: Eq, V> PartialEq for RbTree<T, V> {
    fn eq(&self, other: &Self) -> bool {
        unsafe {
            let mut ptr_a = self.lowest_node_raw().unwrap_or_default();
            let mut ptr_b = other.lowest_node_raw().unwrap_or_default();
            while !ptr_a.is_null() && !ptr_b.is_null() {
                if (*ptr_a).value != (*ptr_b).value {
                    return false;
                }
                ptr_a = (*ptr_a).successor().unwrap_or_default();
                ptr_b = (*ptr_b).successor().unwrap_or_default();
            }
            ptr_a.is_null() && ptr_b.is_null()
        }
    }
}

impl<T: Eq, V> Eq for RbTree<T, V> {}

impl<T, V: Augment<T>> RbTree<T, V> {
    /// calls update_fn on every node in the tree.
    /// 
    /// SAFETY: updates must not disturb the ordering or augments.
    pub unsafe fn update_values(&mut self, mut update_fn: impl FnMut(&mut T)) {
        unsafe {
            let mut node = self.lowest_node_raw().unwrap_or_default();
            while !node.is_null() {
                update_fn(&mut (*node).value);
                node = (*node).successor().unwrap_or_default();
            }
        }
    }

    /// compact the tree by merging adjacent pairs.
    /// merge_fn is called on every pair, if it returns true, the right value is removed.
    /// it is allowed/expected to modify the left value to reflect the merge.
    ///
    /// SAFETY: merge_fn must not change left values in a way that disturbs the ordering.
    /// it should only make changes that disturb augments if it returns true.
    pub unsafe fn pairwise_merge(&mut self, merge_fn: impl Fn(&mut T, &T) -> bool) {
        unsafe {
            let Some(mut node) = self.lowest_node_raw() else {
                return;
            };
            while let Some(succ) = (*node).successor() {
                if merge_fn((*node).value_mut(), (*succ).value()) {
                    self.remove_raw(succ);
                    mark_dirty(node);
                } else {
                    node = succ;
                }
            }
            self.update_augments();
        }
    }
}

#[allow(dead_code)]
mod tests {
    use super::*;
    use crate::memory::direct_alloc;
    use core::alloc::Layout;

    fn print_tree<T: core::fmt::Debug, V: core::fmt::Debug>(node: *mut RbNode<T, V>, depth: i32) {
        unsafe {
            for _ in 0..depth {
                print!("+");
            }
            if color(node) == Red {
                print!("R ");
            } else {
                print!("B ");
            }
            if node.is_null() {
                println!("nil");
            } else {
                println!("{:?} [{:?}]", (*node).value, (*node).augment);
                print_tree((*node).left, depth + 1);
                print_tree((*node).right, depth + 1);
            }
        }
    }

    pub(super) fn tree_check<T, V: Augment<T> + Eq + core::fmt::Debug>(
        node: *mut RbNode<T, V>,
        parent: *mut RbNode<T, V>,
        min: Option<&T>,
        max: Option<&T>,
        cmp: &impl Fn(&T, &T) -> Ordering,
    ) -> (u32, Option<V>) {
        unsafe {
            if node.is_null() {
                return (1, None);
            }
            assert_eq!((*node).parent, parent);
            assert!(color(node) == Black || color((*node).left) == Black);
            assert!(color(node) == Black || color((*node).right) == Black);
            if let Some(min) = min {
                assert!(cmp(&(*node).value, min).is_ge())
            }
            if let Some(max) = max {
                assert!(cmp(&(*node).value, max).is_le())
            }
            let (left_depth, left_augment) =
                tree_check((*node).left, node, min, Some(&(*node).value), cmp);
            let (right_depth, right_augment) =
                tree_check((*node).right, node, Some(&(*node).value), max, cmp);
            assert_eq!(left_depth, right_depth);
            let augment = V::augment(&(*node).value, &left_augment, &right_augment);
            assert_eq!(Some(&augment), (*node).augment.as_ref());
            (left_depth + ((color(node) == Black) as u32), Some(augment))
        }
    }

    fn pick_random<T, V>(rng: &mut fastrand::Rng, node: *mut RbNode<T, V>) -> *mut RbNode<T, V> {
        unsafe {
            loop {
                match rng.i32(0..3) {
                    0 => return node,
                    1 => {
                        if !(*node).left.is_null() {
                            return pick_random(rng, (*node).left);
                        }
                    }
                    2 => {
                        if !(*node).right.is_null() {
                            return pick_random(rng, (*node).right);
                        }
                    }
                    _ => unreachable!(),
                }
            }
        }
    }

    fn kernel_alloc_ptr<T>() -> *mut T {
        direct_alloc(Layout::new::<T>()).unwrap().as_mut_ptr()
    }

    #[derive(Default, Debug, PartialEq, Eq, Clone, Copy)]
    struct Sum(i32);

    impl Augment<i32> for Sum {
        fn augment(node: &i32, left: &Option<Self>, right: &Option<Self>) -> Self {
            Sum(*node + left.unwrap_or_default().0 + right.unwrap_or_default().0)
        }
    }

    #[test_case]
    fn test_rbtree_insertion() {
        let mut rng = fastrand::Rng::with_seed(42);
        for _ in 0..10 {
            let mut tree: RbTree<i32, Sum> = RbTree { head: null_mut() };
            for _ in 0..1000 {
                tree.insert(OwnedRbNode::new(rng.i32(0..100)), Ord::cmp);
                //print_tree(tree.head, 0);
                //serial_println!("---");
                tree_check(tree.head, null_mut(), None, None, &Ord::cmp);
            }
        }
    }

    #[test_case]
    fn test_rbtree_deletions() {
        let mut rng = fastrand::Rng::with_seed(42);
        for _ in 0..10 {
            let mut tree: RbTree<i32, Sum> = RbTree { head: null_mut() };
            for _ in 0..1000 {
                tree.insert(OwnedRbNode::new(rng.i32(0..100)), Ord::cmp);
            }
            while !tree.head.is_null() {
                let node = pick_random(&mut rng, tree.head);
                unsafe { tree.remove_raw(node) };
                //print_tree(tree.head, 0);
                //serial_println!("---");
                tree_check(tree.head, null_mut(), None, None, &Ord::cmp);
            }
        }
    }
}
