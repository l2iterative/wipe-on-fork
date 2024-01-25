use std::sync::Mutex;

pub struct GenerationCounter {
    pub(crate) gen: Mutex<Option<u64>>,
}

impl GenerationCounter {
    pub const fn new() -> Self {
        Self {
            gen: Mutex::new(None),
        }
    }

    pub fn get(&self) -> u64 {
        let mut lock = self.gen.lock().unwrap();
        if lock.is_some() {
            return lock.unwrap();
        } else {
            unsafe {
                libc::pthread_atfork(None, None, Some(update_generations));
            }
            *lock = Some(0u64);
            return 0u64;
        }
    }
}

pub(crate) static GENERATION: GenerationCounter = GenerationCounter::new();

unsafe extern "C" fn update_generations() {
    let mut lock = GENERATION.gen.lock().unwrap();
    if lock.is_some() {
        *lock = Some(lock.unwrap() + 1);
    } else {
        panic!("The generation counter is expected to have started.");
    }
}
