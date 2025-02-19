// Copyright 2025 foyer Project Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::{
    pin::Pin,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use futures::{task::AtomicWaker, Future};

#[derive(Debug, Default)]
struct WaitGroupInner {
    counter: AtomicUsize,
    waker: AtomicWaker,
}

/// A [`WaitGroup`] waits for all acquired [`WaitGroupGuard`] to drop.
#[derive(Debug, Default)]
pub struct WaitGroup {
    inner: Arc<WaitGroupInner>,
}

impl WaitGroup {
    /// Acquire a [`WaitGroupGuard`] for the [`WaitGroup`] to wait for.
    pub fn acquire(&self) -> WaitGroupGuard {
        self.inner.counter.fetch_add(1, Ordering::SeqCst);
        WaitGroupGuard {
            inner: self.inner.clone(),
        }
    }

    /// Consume the [`WaitGroup`] and generate a [`WaitGroupFuture`].
    pub fn wait(self) -> WaitGroupFuture {
        WaitGroupFuture {
            inner: self.inner,
            initialized: false,
        }
    }
}

/// A [`WaitGroupGuard`] is generated by [`WaitGroup::acquire`].
#[derive(Debug)]
pub struct WaitGroupGuard {
    inner: Arc<WaitGroupInner>,
}

impl Drop for WaitGroupGuard {
    fn drop(&mut self) {
        if self.inner.counter.fetch_sub(1, Ordering::SeqCst) - 1 == 0 {
            // Wake up the future if this is the last count.
            //
            // - If the waker is not set yet, this is a no-op. The counter might be increased again later.
            // - If the waker is already set, the counter will be no longer increased, so this is the actual last count.
            self.inner.waker.wake();
        }
    }
}

/// A [`WaitGroupFuture`] is generated by [`WaitGroup::wait`].
///
/// A [`WaitGroupFuture`] will not be ready until all related [`WaitGroupGuard`]s are dropped.
#[must_use]
#[derive(Debug)]
pub struct WaitGroupFuture {
    inner: Arc<WaitGroupInner>,
    initialized: bool,
}

impl Future for WaitGroupFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.initialized {
            self.initialized = true;
            self.inner.waker.register(cx.waker());
        }

        if self.inner.counter.load(Ordering::SeqCst) == 0 {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;

    #[tokio::test]
    async fn test_wait_group_empty() {
        let wg = WaitGroup::default();
        wg.wait().await;
    }

    #[tokio::test]
    async fn test_wait_group_basic() {
        let v = Arc::new(AtomicUsize::new(0));
        let wg = WaitGroup::default();

        let g = wg.acquire();
        let vv = v.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            vv.fetch_add(1, Ordering::SeqCst);
            drop(g);
        });

        sleep(Duration::from_millis(10)).await;
        wg.wait().await;
        assert_eq!(v.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_wait_group_dip_rise() {
        let v = Arc::new(AtomicUsize::new(0));
        let wg = WaitGroup::default();

        let g1 = wg.acquire();
        let vv = v.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(10)).await;
            vv.fetch_add(1, Ordering::SeqCst);
            drop(g1);
        });

        let g2 = wg.acquire();
        let vv = v.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(100)).await;
            vv.fetch_add(1, Ordering::SeqCst);
            drop(g2);
        });

        sleep(Duration::from_millis(50)).await;
        wg.wait().await;
        assert_eq!(v.load(Ordering::SeqCst), 2);
    }
}
