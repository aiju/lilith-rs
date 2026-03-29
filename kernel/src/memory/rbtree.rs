use core::{
    alloc::Layout,
    ptr::{null, null_mut},
    sync::atomic::{AtomicPtr, Ordering},
};

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

use crate::{memory::kernel_alloc, serial_print, serial_println};

impl core::ops::Not for Direction {
    type Output = Direction;

    fn not(self) -> Self::Output {
        match self {
            Left => Right,
            Right => Left,
        }
    }
}

struct RbNode<T> {
    color: Color,
    parent: *mut RbNode<T>,
    left: *mut RbNode<T>,
    right: *mut RbNode<T>,
    value: T,
}

impl<T> RbNode<T> {
    fn child(&self, dir: Direction) -> *mut RbNode<T> {
        match dir {
            Left => self.left,
            Right => self.right,
        }
    }
    fn set_child(&mut self, dir: Direction, value: *mut RbNode<T>) {
        match dir {
            Left => self.left = value,
            Right => self.right = value,
        }
        if !value.is_null() {
            unsafe { (*value).parent = self };
        }
    }
    fn child_dir(&self, child: *mut RbNode<T>) -> Direction {
        assert!(self.left == child || self.right == child);
        if self.left == child { Left } else { Right }
    }
    fn grandparent(&self) -> *mut RbNode<T> {
        unsafe {
            if !self.parent.is_null() {
                (*self.parent).parent
            } else {
                null_mut()
            }
        }
    }
    fn uncle(&self) -> *mut RbNode<T> {
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

struct RbTree<T> {
    head: *mut RbNode<T>,
}

fn color<T>(node: *mut RbNode<T>) -> Color {
    unsafe { if node.is_null() { Black } else { (*node).color } }
}

fn place<T: Ord>(head: *mut *mut RbNode<T>, node: *mut RbNode<T>) {
    unsafe {
        (*node).color = Red;
        (*node).left = null_mut();
        (*node).right = null_mut();

        let mut parent = null_mut();
        let mut link = head;
        while !(*link).is_null() {
            parent = *link;
            if (*node).value <= (*parent).value {
                link = &raw mut (*parent).left;
            } else {
                link = &raw mut (*parent).right;
            }
        }
        (*node).parent = parent;
        *link = node;
    }
}

fn recolor<T>(mut node: *mut RbNode<T>) -> *mut RbNode<T> {
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

fn successor<T>(node: *mut RbNode<T>) -> *mut RbNode<T> {
    unsafe {
        let mut n = (*node).right;
        while !(*n).left.is_null() {
            n = (*n).left;
        }
        n
    }
}

impl<T> RbTree<T>
where
    T: Ord,
{
    fn replace_node(&mut self, node: *mut RbNode<T>, replacement: *mut RbNode<T>) {
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
    fn rotate(&mut self, node: *mut RbNode<T>) {
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
    fn insert(&mut self, node: *mut RbNode<T>) {
        unsafe {
            place(&raw mut self.head, node);
            let w_node = recolor(node);
            if color((*w_node).parent) == Red {
                self.rotate(w_node);
            }
        }
    }
    // removes the node, retaining the correct order
    // returns (parent, child, removed_color) for the location where we broke red-black invariants
    // parent and child may both be null
    fn unplace(&mut self, node: *mut RbNode<T>) -> (*mut RbNode<T>, *mut RbNode<T>, Color) {
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
                    let succ = successor(node);
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
    fn remove_fixup(&mut self, mut parent: *mut RbNode<T>, mut deficit_side: Direction) {
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
    fn remove(&mut self, node: *mut RbNode<T>) {
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
        }
    }
}

fn print_tree<T: core::fmt::Debug>(node: *mut RbNode<T>, depth: i32) {
    unsafe {
        for _ in 0..depth {
            serial_print!("+");
        }
        if color(node) == Red {
            serial_print!("R ");
        } else {
            serial_print!("B ");
        }
        if node.is_null() {
            serial_println!("nil");
        } else {
            serial_println!("{:?}", (*node).value);
            print_tree((*node).left, depth + 1);
            print_tree((*node).right, depth + 1);
        }
    }
}

fn tree_check<T: Ord>(
    node: *mut RbNode<T>,
    parent: *mut RbNode<T>,
    min: Option<&T>,
    max: Option<&T>,
) -> u32 {
    unsafe {
        if node.is_null() {
            return 1;
        }
        assert_eq!((*node).parent, parent);
        assert!(color(node) == Black || color((*node).left) == Black);
        assert!(color(node) == Black || color((*node).right) == Black);
        if let Some(min) = min {
            assert!((*node).value >= *min);
        }
        if let Some(max) = max {
            assert!((*node).value <= *max);
        }
        let left_depth = tree_check((*node).left, node, min, Some(&(*node).value));
        let right_depth = tree_check((*node).right, node, Some(&(*node).value), max);
        assert_eq!(left_depth, right_depth);
        left_depth + ((color(node) == Black) as u32)
    }
}

fn pick_random<T>(rng: &mut fastrand::Rng, node: *mut RbNode<T>) -> *mut RbNode<T> {
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
    kernel_alloc(Layout::new::<T>()).unwrap().as_mut_ptr()
}

#[test_case]
fn test_rbtree_insertion() {
    let mut rng = fastrand::Rng::with_seed(42);
    for _ in 0..10 {
        let mut tree: RbTree<i32> = RbTree { head: null_mut() };
        for _ in 0..1000 {
            let node: *mut RbNode<i32> = kernel_alloc_ptr();
            unsafe { (*node).value = rng.i32(0..100) };
            tree.insert(node);
            tree_check(tree.head, null_mut(), None, None);
        }
    }
}

#[test_case]
fn test_rbtree_deletions() {
    let mut rng = fastrand::Rng::with_seed(42);
    for _ in 0..10 {
        let mut tree: RbTree<i32> = RbTree { head: null_mut() };
        for _ in 0..1000 {
            let node: *mut RbNode<i32> = kernel_alloc_ptr();
            unsafe { (*node).value = rng.i32(0..100) };
            tree.insert(node);
        }
        while !tree.head.is_null() {
            let node = pick_random(&mut rng, tree.head);
            tree.remove(node);
            tree_check(tree.head, null_mut(), None, None);
        }
    }
}
