use std::{cell::RefCell, future::Future, rc::Rc};

use tokio::task;

type QueueNodeRef<T> = Rc<RefCell<QueueNode<T>>>;
type OptQueueNodeRef<T> = Option<QueueNodeRef<T>>;

pub type TreeNodeRef<T> = Rc<RefCell<TreeNode<T>>>;
type OptTreeNodeRef<T> = Option<TreeNodeRef<T>>;
type TraverseFunction<T> = fn(&T);

#[derive(Debug, Default, Clone)]
pub struct TreeNode<T: Default + Clone> {
    pub value: T,
    pub children: Vec<TreeNodeRef<T>>,
}

#[derive(Debug, Default, Clone)]
pub struct Tree<T: Default + Clone> {
    pub root: TreeNodeRef<T>,
    pub depth: usize,
}

#[derive(Debug, Default)]
struct QueueNode<T: Default> {
    pub value: T,
    pub next: OptQueueNodeRef<T>,
}

#[derive(Debug, Default, Clone)]
pub struct Queue<T: Default + Clone> {
    head: OptQueueNodeRef<T>,
    tail: OptQueueNodeRef<T>,
    pub length: usize,
}

impl<T: Default + Clone> Tree<T> {
    pub fn push_node(parent: TreeNodeRef<T>, child: TreeNodeRef<T>) {
        parent.borrow_mut().children.push(child);
    }

    pub async fn traverse_async<F, Fut>(&self, mut f: F)
    where
        T: Send + 'static,
        F: FnMut(T) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static
    {
        let mut q = Queue::default();
        q.push(self.root.clone());
        // handles
        let mut h = vec![];

        while !q.is_empty() {
            if let Some(current) = q.pop() {
                let b = current.borrow().clone();
                let value = b.value;
                let children = b.children;

                for child in children {
                    q.push(child);
                }
                h.push(task::spawn(f(value)));
            }
        }
        for handle in h {
            handle.await.unwrap();
        }
    }

    pub fn traverse<F>(&self, mut f: F)
    where
        F: FnMut(&T),
        T: Default,
    {
        let mut q = Queue::default();
        q.push(self.root.clone());
        while !q.is_empty() {
            if let Some(current) = q.pop() {
                let borrowed = current.borrow();
                let children = borrowed.children.clone();
                let value = &borrowed.value;
                for child in children {
                    q.push(child);
                }
                f(value);
            }
        }
    }

    pub fn new(root: TreeNode<T>) -> Self
    where
        T: Default,
    {
        Self {
            root: Rc::new(RefCell::new(root)),
            depth: 1,
        }
    }
}

impl<T: Default + Clone> TreeNode<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            ..Self::default()
        }
    }
}

impl<T: Default + Clone> Queue<T> {
    pub fn push(&mut self, value: T)
    where
        T: Default,
    {
        let new = Rc::new(RefCell::new(QueueNode::new(value)));
        if self.head.is_none() && self.tail.is_none() {
            let newc = Rc::clone(&new);
            self.head = Some(new);
            self.tail = Some(newc);
            self.length += 1;
            return;
        }

        // Note: this is a funny thing lets unwrap
        // you cannot:
        // self.tail.unwrap()... here because this would look like this written out
        // let tail = self.tail;
        // let unwraped = tail.unwrap().borrow_mut();
        // and unwrap takes a reference and borrow wants also a mutable reference...
        // you can however:
        // if let Some(tail_rc) = self.tail.clone() {
        //     tail_rc.borrow_mut().next = Some(new_node.clone());
        // }
        // self.tail = Some(new_node);
        // ultimate killer thing is the take because it moves the tail out then unwraps then
        // borrows mut the queue tail is then set to None and in the next line we set the queue
        // tail to the new value
        self.tail.take().unwrap().borrow_mut().next = Some(new.clone());
        self.tail = Some(new);
        self.length += 1;
    }
    pub fn pop(&mut self) -> Option<T>
    where
        T: Default,
    {
        if self.head.is_none() || self.tail.is_none() {
            assert!(self.head.is_none() && self.tail.is_none());
            return None;
        }

        self.length -= 1;
        let old_head = self.head.take().unwrap();
        let node = old_head.take();
        self.head = node.next;

        if self.head.is_none() {
            assert!(self.length == 0);
            self.tail = None;
        }

        Some(node.value)
    }
    pub fn is_empty(&self) -> bool {
        self.head.is_none() && self.tail.is_none()
    }
}

impl<T: Default> QueueNode<T> {
    pub fn new(value: T) -> QueueNode<T>
    where
        T: Default,
    {
        QueueNode {
            value,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod test {
    use std::{cell::RefCell, rc::Rc};

    use super::{Queue, QueueNode, Tree, TreeNode};

    #[test]
    fn test_default() {
        let d: Queue<usize> = Queue::default();
        let n: QueueNode<usize> = QueueNode::default();
        assert!(d.tail.is_none());
        assert!(d.head.is_none());
        assert_eq!(d.length, 0);
        assert!(n.next.is_none());
        assert_eq!(n.value, 0);
    }

    #[test]
    fn test_push_queue() {
        let new = 10;
        let mut q: Queue<usize> = Queue::default();
        assert!(q.is_empty());
        q.push(new);
        assert!(!q.is_empty());
        assert_eq!(q.tail.unwrap().borrow().value, new);
    }

    #[test]
    fn test_pop_queue() {
        let mut q: Queue<usize> = Queue::default();
        q.push(10);
        q.push(42);
        q.push(3);
        let ten = q.pop().expect("This should be 3");
        assert!(ten == 10);
        let foutytwo = q.pop().expect("This should be 42");
        assert!(foutytwo == 42);
        q.push(69);
        let _ = q.pop();
        let _ = q.pop();
        assert!(q.is_empty());
        assert!(q.head.is_none());
        assert!(q.tail.is_none());
        let none = q.pop();
        assert!(none.is_none());
    }

    #[test]
    fn test_default_tree() {
        let root = TreeNode::new(10);
        let t: Tree<usize> = Tree::new(root);
        assert!(t.root.borrow().value == 10);
    }

    #[test]
    fn test_queue_traverse() {
        let root = TreeNode::new(10);
        let t: Tree<usize> = Tree::new(root);

        let refc1 = Rc::new(RefCell::new(TreeNode::new(1)));
        let refc2 = Rc::new(RefCell::new(TreeNode::new(2)));
        let refc3 = Rc::new(RefCell::new(TreeNode::new(3)));
        let refc4 = Rc::new(RefCell::new(TreeNode::new(4)));
        let refc5 = Rc::new(RefCell::new(TreeNode::new(5)));
        let refc6 = Rc::new(RefCell::new(TreeNode::new(6)));
        let refc7 = Rc::new(RefCell::new(TreeNode::new(7)));
        let refc8 = Rc::new(RefCell::new(TreeNode::new(8)));
        let refc9 = Rc::new(RefCell::new(TreeNode::new(9)));

        Tree::push_node(t.root.clone(), refc1.clone());
        Tree::push_node(refc1.clone(), refc2.clone());
        Tree::push_node(refc1.clone(), refc3);
        Tree::push_node(refc1, refc4.clone());
        Tree::push_node(refc2, refc5.clone());
        Tree::push_node(refc4.clone(), refc6);
        Tree::push_node(refc4, refc7);
        Tree::push_node(refc5, refc8.clone());
        Tree::push_node(refc8, refc9);

        let nodes = Rc::new(RefCell::new(Vec::new()));
        let clone = nodes.clone();

        t.traverse(move |n| clone.borrow_mut().push(*n));
        assert_eq!(nodes.take(), vec![10, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }
}
