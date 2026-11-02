use std::cell::RefCell;

thread_local! {
    static AFTER_ROOT: RefCell<Option<Box<dyn FnOnce()>>> = const { RefCell::new(None) };
}

pub(crate) fn after_root_validation() {
    let hook = AFTER_ROOT.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

pub(crate) fn on_root_validated(hook: impl FnOnce() + 'static) {
    AFTER_ROOT.with(|slot| {
        assert!(
            slot.borrow().is_none(),
            "同一线程不能覆盖尚未执行的目录交换门闩"
        );
        *slot.borrow_mut() = Some(Box::new(hook));
    });
}
