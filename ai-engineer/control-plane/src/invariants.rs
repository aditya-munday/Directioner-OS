use std::marker::PhantomData;

pub struct Draft;
pub struct Reviewed;
pub struct Approved;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecyclePlan<State> {
    pub plan_id: String,
    pub state: PhantomData<State>,
}

impl LifecyclePlan<Draft> {
    pub fn new(plan_id: impl Into<String>) -> Self {
        Self {
            plan_id: plan_id.into(),
            state: PhantomData,
        }
    }

    pub fn mark_reviewed(self) -> LifecyclePlan<Reviewed> {
        LifecyclePlan {
            plan_id: self.plan_id,
            state: PhantomData,
        }
    }
}

impl LifecyclePlan<Reviewed> {
    pub fn mark_approved(self) -> LifecyclePlan<Approved> {
        LifecyclePlan {
            plan_id: self.plan_id,
            state: PhantomData,
        }
    }
}

#[allow(clippy::assertions_on_constants)]
const _: () = assert!(20 <= 40);
#[allow(clippy::assertions_on_constants)]
const _: () = assert!(40 <= 80);
const _: () = assert!(std::mem::size_of::<usize>() >= 4);
