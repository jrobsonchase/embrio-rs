#![no_std]
#![feature(generators)]

use core::{
    future::Future,
    task::{Context, Poll},
};

use pin_utils::pin_mut;

use futures::task::noop_waker;

use embrio_async::{async_block, async_fn};

fn block_on<F>(f: F) -> <F as Future>::Output
where
    F: Future,
{
    pin_mut!(f);

    loop {
        match f.as_mut().poll(&mut Context::from_waker(&noop_waker())) {
            Poll::Pending => continue,
            Poll::Ready(n) => {
                return n;
            }
        }
    }
}

#[test]
fn test_async_block() {
    let f = async_block! {
        5usize
    };

    let f2 = async_block! {
        ewait!(f)
    };

    assert_eq!(block_on(f2), 5);
}

#[derive(Eq, PartialEq, Debug)]
enum Either<L, R> {
    Left(L),
    Right(R),
}

#[async_fn]
fn a_number_and_string<'a, 'b>(
    n: &'a usize,
    s: &'b str,
) -> Either<usize, &'b str> {
    if *n % 2 == 0 {
        Either::Left(*n)
    } else {
        Either::Right(s)
    }
}

#[async_fn]
fn a_wait_thing() -> Either<usize, &'static str> {
    ewait!(a_number_and_string(&5, "Hello, world!"))
}

#[test]
fn test_async_fn() {
    assert_eq!(block_on(a_wait_thing()), Either::Right("Hello, world!"));
}
