use std::{any::TypeId, hash::BuildHasherDefault};

use ahash::AHasher;
use gc_arena::{
    allocator_api::MetricsAlloc, lock::RefLock, Collect, DynamicRootSet, Gc, Mutation, Root,
    Rootable,
};
use hashbrown::{hash_map, HashMap};

use crate::{
    any::Any,
    stash::{Fetchable, Stashable},
    Context,
};

pub trait Singleton<'gc> {
    fn create(ctx: Context<'gc>) -> Self;
}

impl<'gc, T: Default> Singleton<'gc> for T {
    fn create(_: Context<'gc>) -> Self {
        Self::default()
    }
}

#[derive(Copy, Clone, Collect)]
#[collect(no_drop)]
pub struct Registry<'gc> {
    roots: DynamicRootSet<'gc>,
    singletons:
        Gc<'gc, RefLock<HashMap<TypeId, Any<'gc>, BuildHasherDefault<AHasher>, MetricsAlloc<'gc>>>>,
}

impl<'gc> Registry<'gc> {
    pub fn new(mc: &Mutation<'gc>) -> Self {
        let singletons =
            HashMap::with_hasher_in(BuildHasherDefault::default(), MetricsAlloc::new(mc));

        Self {
            roots: DynamicRootSet::new(mc),
            singletons: Gc::new(mc, RefLock::new(singletons)),
        }
    }

    pub fn roots(&self) -> DynamicRootSet<'gc> {
        self.roots
    }

    /// Create an instance of a type that exists at most once per `Lua` instance.
    ///
    /// If the type has already been created, returns the already created instance, otherwise calls
    /// `S::create` to create a new instance and returns it.
    pub fn singleton<S>(&self, ctx: Context<'gc>) -> &'gc Root<'gc, S>
    where
        S: for<'a> Rootable<'a>,
        Root<'gc, S>: Singleton<'gc>,
    {
        let mut singletons = self.singletons.borrow_mut(&ctx);
        match singletons.entry(TypeId::of::<S>()) {
            hash_map::Entry::Occupied(occupied) => occupied.get().downcast::<S>().unwrap(),
            hash_map::Entry::Vacant(vacant) => {
                let v = Root::<'gc, S>::create(ctx);
                vacant
                    .insert(Any::new::<S>(&ctx, v))
                    .downcast::<S>()
                    .unwrap()
            }
        }
    }

    /// "Stash" a value with a `'gc` branding lifetime, creating a `'static` handle to it.
    ///
    /// This is a wrapper around an internal `gc-arena::DynamicRootSet` that makes it a little
    /// simpler to work with common piccolo types without having to manually specify `Rootable`
    /// projections.
    ///
    /// It can be implemented for external types by implementing the `Stashable` trait.
    pub fn stash<S: Stashable<'gc>>(&self, mc: &Mutation<'gc>, s: S) -> S::Stashed {
        s.stash(mc, self.roots)
    }

    /// "Fetch" the real value for a handle that has been returned from `Registry::stash`.
    ///
    /// It can be implemented for external types by implementing the `Fetchable` trait.
    pub fn fetch<F: Fetchable<'gc>>(&self, f: &F) -> F::Fetched {
        f.fetch(self.roots)
    }
}
