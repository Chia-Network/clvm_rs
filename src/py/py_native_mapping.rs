use pyo3::prelude::*;
//use pyo3::pyclass::PyClassAlloc;`
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

use super::py_na_node::PyNaNode;

/*
pub trait PyNativeMapping {
    fn add(
        &self,
        py: Python,
        obj: &PyCell<PyNaNode>,
        ptr: &<IntAllocator as Allocator>::Ptr,
    ) -> PyResult<()>;

    fn native_for_py(
        &self,
        py: Python,
        obj: &PyCell<PyNaNode>,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr>;

    fn py_for_native<'p>(
        &'p self,
        py: Python<'p>,
        ptr: &<IntAllocator as Allocator>::Ptr,
        allocator: &mut IntAllocator,
    ) -> PyResult<&'p PyCell<PyNaNode>>;
}
*/

pub fn new_mapping(py: Python) -> PyResult<PyObject> {
    Ok(py
        //.eval("__import__('weakref').WeakValueDictionary()", None, None)?
        .eval("dict()", None, None)?
        .to_object(py))
}

pub fn add(
    py: Python,
    cache: &PyObject,
    obj: &PyCell<PyNaNode>,
    ptr: &<IntAllocator as Allocator>::Ptr,
) -> PyResult<()> {
    //obj.borrow().int_cache.set(Some(ptr.clone()));

    let locals = [
        ("cache", cache.clone()),
        ("obj", obj.to_object(py)),
        ("ptr", ptr.to_object(py)),
    ]
    .into_py_dict(py);

    py.run("cache[ptr] = obj; cache[obj] = ptr", None, Some(locals))
}

// py to native methods

fn from_py_to_native_cache<'p>(
    py: Python<'p>,
    cache: &PyObject,
    obj: &PyCell<PyNaNode>,
) -> PyResult<<IntAllocator as Allocator>::Ptr> {
    let locals = [("cache", cache.clone()), ("key", obj.to_object(py))].into_py_dict(py);
    py.eval("cache.get(key)", None, Some(locals))?.extract()
}

fn populate_native(
    py: Python,
    cache: &PyObject,
    obj: &PyCell<PyNaNode>,
    allocator: &mut IntAllocator,
) -> PyResult<<IntAllocator as Allocator>::Ptr> {
    apply_to_tree(obj.to_object(py), move |obj| {
        let node: &PyCell<PyNaNode> = obj.extract(py)?;

        // is it in cache yet?
        if from_py_to_native_cache(py, cache, node).is_ok() {
            // yep, we're done
            return Ok(None);
        }

        // it's not in the cache

        match &node.borrow().py_view {
            Some(PyView::Atom(obj)) => {
                let blob: &[u8] = obj.extract(py).unwrap();
                let ptr = allocator.new_atom(blob).unwrap();
                add(py, cache, node, &ptr)?;

                Ok(None)
            }
            Some(PyView::Pair(pair)) => {
                let pair: &PyAny = pair.clone().into_ref(py);
                let pair: &PyTuple = pair.extract()?;
                let p0: &PyCell<PyNaNode> = pair.get_item(0).extract()?;
                let p1: &PyCell<PyNaNode> = pair.get_item(1).extract()?;
                let ptr_0: PyResult<i32> = from_py_to_native_cache(py, cache, p0);
                let ptr_1: PyResult<i32> = from_py_to_native_cache(py, cache, p1);
                if let (Ok(ptr_0), Ok(ptr_1)) = (ptr_0, ptr_1) {
                    let ptr = allocator.new_pair(ptr_0, ptr_1).unwrap();
                    add(py, cache, node, &ptr)?;
                    Ok(None)
                } else {
                    Ok(Some((p0.to_object(py), p1.to_object(py))))
                }
            }
            _ => py_raise(py, "py view is None"),
        }
    })?;

    from_py_to_native_cache(py, cache, obj)
}

pub fn native_for_py(
    py: Python,
    cache: &PyObject,
    obj: &PyCell<PyNaNode>,
    allocator: &mut IntAllocator,
) -> PyResult<<IntAllocator as Allocator>::Ptr> {
    from_py_to_native_cache(py, cache, obj)
        .or_else(|err| populate_native(py, cache, obj, allocator))
}

// native to py methods

fn from_native_to_py_cache<'p>(
    py: Python<'p>,
    cache: &PyObject,
    ptr: &<IntAllocator as Allocator>::Ptr,
) -> PyResult<&'p PyCell<PyNaNode>> {
    let locals = [("cache", cache.clone()), ("key", ptr.to_object(py))].into_py_dict(py);
    py.eval("cache[key]", None, Some(locals))?.extract()
}

fn populate_python<'p>(
    py: Python<'p>,
    cache: &PyObject,
    ptr: &<IntAllocator as Allocator>::Ptr,
    allocator: &mut IntAllocator,
) -> PyResult<&'p PyCell<PyNaNode>> {
    apply_to_tree(ptr.clone(), move |ptr| {
        // is it in cache yet?
        if from_native_to_py_cache(py, cache, &ptr).is_ok() {
            // yep, we're done
            return Ok(None);
        }

        // it's not in the cache

        match allocator.sexp(&ptr) {
            SExp::Atom(a) => {
                // it's an atom, so we just populate cache directly
                let blob = allocator.buf(&a);
                let py_bytes = PyBytes::new(py, blob);
                add(
                    py,
                    cache,
                    PyNaNode::new(py, Some(PyView::new_atom(py, py_bytes)))?,
                    &ptr,
                )?;
                Ok(None)
            }
            SExp::Pair(ptr_1, ptr_2) => {
                // we can only create this if the children are in the cache
                // Let's fine out
                let locals = [
                    ("cache", cache.clone()),
                    ("p1", ptr_1.to_object(py)),
                    ("p2", ptr_2.to_object(py)),
                ]
                .into_py_dict(py);

                let pair: PyResult<&PyAny> = py.eval("(cache[p1], cache[p2])", None, Some(locals));

                match pair {
                    // the children aren't in the cache, keep drilling down
                    Err(_) => Ok(Some((ptr_1, ptr_2))),

                    // the children are in the cache, create new node & populate cache with it
                    Ok(tuple) => {
                        let (p1, p2): (&PyCell<PyNaNode>, &PyCell<PyNaNode>) = tuple.extract()?;
                        add(
                            py,
                            cache,
                            PyNaNode::new(
                                py,
                                Some(PyView::new_pair(py, PyTuple::new(py, &[p1, p2]))?),
                            )?,
                            &ptr,
                        )?;
                        Ok(None)
                    }
                }
            }
        }
    })?;

    from_native_to_py_cache(py, cache, &ptr)
}

pub fn py_for_native<'p>(
    py: Python<'p>,
    cache: &PyObject,
    ptr: &<IntAllocator as Allocator>::Ptr,
    allocator: &mut IntAllocator,
) -> PyResult<&'p PyCell<PyNaNode>> {
    from_native_to_py_cache(py, cache, ptr)
        .or_else(|err| Ok(populate_python(py, cache, ptr, allocator)?))
}

fn apply_to_tree<T, F>(mut node: T, mut apply: F) -> PyResult<()>
where
    F: FnMut(T) -> PyResult<Option<(T, T)>>,
    T: Clone,
{
    let mut items = vec![node];
    loop {
        let t = items.pop();
        if let Some(obj) = t {
            if let Some((p0, p1)) = apply(obj.clone())? {
                items.push(obj);
                items.push(p0);
                items.push(p1);
            }
        } else {
            break;
        }
    }
    Ok(())
}

fn py_raise<T>(py: Python, msg: &str) -> PyResult<T> {
    let locals = [("msg", msg.to_object(py))].into_py_dict(py);

    py.run("raise RuntimeError(msg)", None, Some(locals))?;
    panic!("we should never get here")
}
