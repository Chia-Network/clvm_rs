use std::borrow::Borrow;
use std::cell::{Ref, RefCell};

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::native_view::NativeView;
use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

#[derive(Clone)]
enum View {
    Python(PyView),
    Native(NativeView),
}

#[pyclass(weakref, subclass)]
pub struct PyNaNode {
    py_view: Option<PyView>,
    native_view: Option<NativeView>,
    //int_arena_cache: PyObject, // WeakKeyDict[PyIntAllocator, int]
}

pub fn new_cache(py: Python) -> PyResult<PyObject> {
    Ok(py
        .eval("__import__('weakref').WeakValueDictionary()", None, None)?
        .to_object(py))
}

fn add_to_cache(
    py: Python,
    cache: &PyObject,
    ptr: <IntAllocator as Allocator>::Ptr,
    value: &PyCell<PyNaNode>,
) -> PyResult<()> {
    //return Ok(());
    let locals = [
        ("cache", cache.clone()),
        ("key", ptr.to_object(py)),
        ("value", value.to_object(py)),
    ]
    .into_py_dict(py);

    Ok(py.run("cache[key] = value", None, Some(locals))?)
}

fn from_cache(
    py: Python,
    cache: &PyObject,
    ptr: <IntAllocator as Allocator>::Ptr,
) -> PyResult<Option<PyObject>> {
    println!("cc 0");
    let locals = [("cache", cache.clone()), ("key", ptr.to_object(py))].into_py_dict(py);
    println!("cc 1 {}", locals);
    let r = py.eval("cache.get(key)", None, Some(locals))?.extract();
    println!("cc 2");
    r
}

impl PyNaNode {
    fn new(
        py: Python,
        py_view: Option<PyView>,
        native_view: Option<NativeView>,
    ) -> PyResult<&PyCell<Self>> {
        PyCell::new(
            py,
            PyNaNode {
                py_view,
                native_view,
            },
        )
    }

    pub fn clear_native_view(slf: &PyCell<Self>, py: Python) -> PyResult<()> {
        let mut items = vec![slf.to_object(py)];
        loop {
            let t = items.pop();
            if let Some(obj) = t {
                let mut node: PyRefMut<Self> = obj.extract(py)?;
                node.populate_python_view(py)?;
                assert!(node.py_view.is_some());
                node.native_view = None;
                if let Some(PyView::Pair(tuple)) = &node.py_view {
                    let (p0, p1): (PyObject, PyObject) = tuple.extract(py)?;
                    //let (p0, p1): (&PyCell<Self>, &PyCell<Self>) = tuple.extract(py)?;
                    items.push(p0);
                    items.push(p1);
                }
            } else {
                break;
            }
        }
        Ok(())
    }

    pub fn add_to_cache(slf: &PyCell<Self>, py: Python, cache: &PyObject) -> PyResult<()> {
        if let Some(native_view) = &slf.borrow().native_view {
            add_to_cache(py, cache, native_view.ptr, slf)?;
        }
        Ok(())
    }

    pub fn from_ptr<'p>(
        py: Python<'p>,
        py_int_allocator: &PyObject,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyCell<Self>> {
        let native_view = NativeView::new(py_int_allocator.clone(), ptr);
        Self::new(py, None, Some(native_view))
    }

    pub fn ptr(
        slf: &PyCell<Self>,
        py: Python,
        cache: &PyObject,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        // check if we need to clear the native view
        // if arena doesn't match, clear native view
        println!("ptr 4");
        Self::populate_native_view(slf, py, cache, arena, allocator)?;
        println!("ptr exiting 1");
        if let Some(native_view) = &slf.borrow().native_view {
            Ok(native_view.ptr)
        } else {
            py_raise(py, "oops")?
        }
    }

    pub fn populate_native_view<'p>(
        slf: &PyCell<Self>,
        py: Python<'p>,
        cache: &PyObject,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<()> {
        let mut to_cast: Vec<PyObject> = vec![slf.to_object(py)];
        Ok(loop {
            println!("pnv vec size {}", to_cast.len());
            let t: Option<PyObject> = to_cast.pop();
            match t {
                None => break,
                Some(node_ref) => {
                    println!("envc 1");
                    let t1: &PyCell<Self> = node_ref.extract(py)?;
                    let transfer: Option<(PyObject, PyObject)> =
                        Self::add_to_native_cache(t1, py, arena, cache, allocator)?;
                    if let Some((p0, p1)) = transfer {
                        to_cast.push(node_ref);
                        println!("p0 borrow");
                        to_cast.push(p0.to_object(py));
                        println!("p1 borrow");
                        to_cast.push(p1.to_object(py));
                        println!("p1 borrowed");
                    }
                }
            }
        })
    }

    /// This instance has a corresponding rep in some `IntAllocator`
    /// Notate this in the cache.
    fn add_to_native_cache<'p>(
        slf_cell: &PyCell<Self>,
        py: Python<'p>,
        arena: &PyObject,
        cache: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<Option<(PyObject, PyObject)>> {
        // if it's an atom, we add it to the allocator & cache the addition
        // if it's a pair, and BOTH are in the cache, we add to allocator & cache
        //  otherwise, we return both children so they can be cached (if necessary)
        let mut slf = slf_cell.borrow_mut();
        let slf: &mut PyNaNode = &mut slf;
        if slf.native_view.is_none() {
            println!("atnc 1");
            let py_view = slf.populate_python_view(py)?;
            println!("atnc 1.5");
            let new_ptr = {
                match py_view {
                    PyView::Atom(obj) => {
                        println!("atnc 2");
                        let blob: &[u8] = obj.extract(py).unwrap();
                        let ptr = allocator.new_atom(blob).unwrap();
                        println!("atnc 3 {}", ptr);
                        add_to_cache(py, cache, ptr, slf_cell);
                        ptr
                    }
                    PyView::Pair(pair) => {
                        println!("atnc 13");
                        let pair: &'p PyAny = pair.clone().into_ref(py);
                        let pair: &'p PyTuple = pair.extract()?;

                        println!("atnc 15");

                        let p0: &'p PyCell<PyNaNode> = pair.get_item(0).extract()?;
                        let p1: &'p PyCell<PyNaNode> = pair.get_item(1).extract()?;
                        let ptr_0 = match &p0.borrow().native_view {
                            Some(native_view) => Some(native_view.ptr),
                            None => None,
                        };
                        let ptr_1 = match &p1.borrow().native_view {
                            Some(native_view) => Some(native_view.ptr),
                            None => None,
                        };
                        println!("atnc 17 {:?} {:?}", ptr_0, ptr_1);
                        if let (Some(ptr_0), Some(ptr_1)) = (ptr_0, ptr_1) {
                            let ptr = allocator.new_pair(ptr_0, ptr_1).unwrap();
                            println!("atnc 18 {}", ptr);
                            add_to_cache(py, cache, ptr, slf_cell);
                            ptr
                        } else {
                            println!("atnc 19");
                            return Ok(Some((p0.to_object(py), p1.to_object(py))));
                        }
                    }
                }
            };
            slf.native_view = Some(NativeView::new(arena.clone(), new_ptr));
            Ok(None)
        } else {
            Ok(None)
        }
    }

    /// If this instance is using `NativeView`, replace it with an equivalent `PythonView`
    /// so it can be use from python.
    pub fn populate_python_view<'p>(&mut self, py: Python<'p>) -> PyResult<&PyView> {
        // if using `NativeView`, swap it out for `PythonView`
        println!("ppv 1");
        if self.py_view.is_none() {
            println!("ppv 2");
            if let Some(native_view) = &self.native_view {
                println!("ppv 3");
                //let mut py_int_allocator: PyRefMut<PyIntAllocator> =
                // native_view.arena.extract(py)?;
                println!("ppv 4");
                //let mut allocator_to_use: &mut IntAllocator = &mut py_int_allocator.arena;
                println!("ppv 5");
                self.py_view = Some(Self::py_view_for_native_view(py, native_view)?);
                println!("ppv 6");
            } else {
                panic!("missing native AND python view");
            }
        }
        println!("ppv 40");
        match &self.py_view {
            Some(py_view) => return Ok(&py_view),
            None => (),
        };
        println!("ppv 41");
        py_raise(py, "no pyview available")?
    }

    fn py_view_for_native_view(py: Python, native_view: &NativeView) -> PyResult<PyView> {
        let mut py_int_allocator: PyRefMut<PyIntAllocator> = native_view.arena.extract(py)?;
        let mut allocator: &mut IntAllocator = &mut py_int_allocator.arena;

        // create a PyView and return it
        let py_view = match allocator.sexp(&native_view.ptr) {
            SExp::Atom(a) => {
                let blob = allocator.buf(&a);
                let py_bytes = PyBytes::new(py, blob);
                PyView::new_atom(py, py_bytes)
            }
            SExp::Pair(ptr_1, ptr_2) => {
                let p1 = Self::from_ptr(py, &native_view.arena, ptr_1)?;
                let p2 = Self::from_ptr(py, &native_view.arena, ptr_2)?;
                PyView::new_pair(py, PyTuple::new(py, &[p1, p2]))?
            }
        };
        Ok(py_view)
    }
}

#[pymethods]
impl PyNaNode {
    #[new]
    fn new_obj<'p>(py: Python<'p>, obj: &PyAny) -> PyResult<Self> {
        Ok(if let Ok(tuple) = obj.extract() {
            let py_view = PyView::new_pair(py, tuple)?;
            Self {
                py_view: Some(py_view),
                native_view: None,
            }
        } else {
            let py_bytes: &PyBytes = obj.extract()?;
            let py_view = PyView::new_atom(py, py_bytes);
            Self {
                py_view: Some(py_view),
                native_view: None,
            }
        })
    }

    #[classmethod]
    fn new_atom<'p>(cls: &PyType, py: Python<'p>, atom: &PyBytes) -> PyResult<&'p PyCell<Self>> {
        let py_view = PyView::new_atom(py, atom);
        Self::new(py, Some(py_view), None)
    }

    #[classmethod]
    fn new_pair<'p>(
        cls: &PyType,
        py: Python<'p>,
        p1: &PyCell<PyNaNode>,
        p2: &PyCell<PyNaNode>,
    ) -> PyResult<&'p PyCell<Self>> {
        let tuple = PyTuple::new(py, &[p1, p2]);
        let py_view = PyView::new_pair(py, tuple)?;
        Self::new(py, Some(py_view), None)
    }

    #[classmethod]
    fn new_tuple<'p>(cls: &PyType, py: Python<'p>, tuple: &PyTuple) -> PyResult<&'p PyCell<Self>> {
        let py_view = PyView::new_pair(py, tuple)?;
        Self::new(py, Some(py_view), None)
    }

    #[getter(atom)]
    pub fn atom<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        println!("atom1");
        let mut slf = slf.try_borrow_mut()?;
        println!("atom2");
        let py_view: &PyView = slf.populate_python_view(py)?;
        println!("atom3");
        match py_view {
            PyView::Atom(obj) => Ok(obj.clone()),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }

    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        let mut slf = slf.try_borrow_mut()?;
        let py_view = slf.populate_python_view(py)?;
        match py_view {
            PyView::Pair(obj) => Ok(obj.clone()),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }
}

fn py_raise<T>(py: Python, msg: &str) -> PyResult<T> {
    let locals = [("msg", msg.to_object(py))].into_py_dict(py);

    py.run("raise RuntimeError(msg)", None, Some(locals))?;
    panic!("we should never get here")
}
