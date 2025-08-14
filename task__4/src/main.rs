#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use rusty_termcolor::{
    colors::{GREEN, MAGENTA},
    effects::{EffectSettings, typewriter},
    formatting::println_colored,
};

mod lazy_iterator {
    // Custom iterator struct that wraps any iterator and extends it with lazy operations
    pub struct LazyIterator<I> {
        pub(crate) inner: I,
    }

    impl<I> LazyIterator<I>
    where
        I: Iterator,
    {
        // Create a new LazyIterator from any iterator
        pub const fn new(iter: I) -> Self {
            Self { inner: iter }
        }

        // Lazy map operation - returns a new iterator that applies the function when consumed
        pub fn lazy_map<F, B>(self, f: F) -> LazyIterator<std::iter::Map<I, F>>
        where
            F: FnMut(I::Item) -> B,
        {
            LazyIterator::new(self.inner.map(f))
        }

        // Lazy filter operation - returns a new iterator that filters when consumed
        pub fn lazy_filter<P>(self, predicate: P) -> LazyIterator<std::iter::Filter<I, P>>
        where
            P: FnMut(&I::Item) -> bool,
        {
            LazyIterator::new(self.inner.filter(predicate))
        }

        // Consume the iterator and collect results
        pub fn collect<B: FromIterator<I::Item>>(self) -> B {
            self.inner.collect()
        }

        // Execute the iterator and perform an action on each item
        pub fn for_each<F>(self, f: F)
        where
            F: FnMut(I::Item),
        {
            self.inner.for_each(f);
        }
    }

    // Implement Iterator trait to make it work with for loops and other iterator methods
    impl<I> Iterator for LazyIterator<I>
    where
        I: Iterator,
    {
        type Item = I::Item;

        fn next(&mut self) -> Option<Self::Item> {
            self.inner.next()
        }
    }

    // Trait to extend collections with lazy iterator capability
    pub trait IntoLazyIterator<T> {
        fn into_lazy_iter(self) -> LazyIterator<std::vec::IntoIter<T>>;
    }

    impl<T> IntoLazyIterator<T> for Vec<T> {
        fn into_lazy_iter(self) -> LazyIterator<std::vec::IntoIter<T>> {
            LazyIterator::new(self.into_iter())
        }
    }
}

use lazy_iterator::IntoLazyIterator;

fn type_write(value: &str) {
    typewriter(
        &format!("\n 🎺 {value:?} 🎺"),
        &EffectSettings::default(),
        Some(&GREEN),
    );
}

fn main() {
    type_write("Lazy Iterator Demonstration");

    // Create a collection
    let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    println_colored(&format!("\n\tNumbers: {numbers:?}"), &MAGENTA);

    // Create lazy iterator with chained operations
    // Note: No computation happens here, just building the iterator chain
    let lazy_result = numbers
        .into_lazy_iter()
        .lazy_filter(|&x| {
            println!("\t\tFiltering: {x}"); // This will show when filtering actually happens
            x % 2 == 0
        })
        .lazy_map(|x| {
            println!("\t\tMapping: {} -> {}", x, x * x); // This will show when mapping actually happens
            x * x
        });

    println_colored("\tLazy iterator created (not yet computed)", &GREEN);

    // Now the computation happens when we collect
    println_colored("\tCollecting results...", &GREEN);
    let results: Vec<i32> = lazy_result.collect();
    println_colored(&format!("\tFinal results: {results:?}"), &MAGENTA);

    // Another example with early termination

    type_write("Early Termination Example");
    let numbers2 = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let first_two_squares: Vec<i32> = numbers2
        .into_lazy_iter()
        .lazy_filter(|&x| x > 3)
        .lazy_map(|x| x * x)
        .take(2) // Only take first 2 items
        .collect();

    println_colored(
        &format!("\n\tFirst two squares > 3: {first_two_squares:?}"),
        &MAGENTA,
    );

    // Example using for_each instead of collect

    type_write("For Each Example");
    let numbers3 = vec![1, 2, 3, 4, 5];

    println_colored("\n\tProcessing each item with for_each:", &MAGENTA);
    numbers3
        .into_lazy_iter()
        .lazy_filter(|&x| x % 2 == 1) // Filter odd numbers
        .lazy_map(|x| x * 3) // Multiply by 3
        .for_each(|x| println!("\t\tProcessed value: {x}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazy_iterator_map() {
        let numbers = vec![1, 2, 3, 4, 5];
        let results: Vec<i32> = numbers.into_lazy_iter().lazy_map(|x| x * 2).collect();
        assert_eq!(results, vec![2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_lazy_iterator_filter() {
        let numbers = vec![1, 2, 3, 4, 5, 6];
        let results: Vec<i32> = numbers
            .into_lazy_iter()
            .lazy_filter(|&x| x % 2 == 0)
            .collect();
        assert_eq!(results, vec![2, 4, 6]);
    }

    #[test]
    fn test_lazy_iterator_chained_operations() {
        let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let results: Vec<i32> = numbers
            .into_lazy_iter()
            .lazy_filter(|&x| x % 2 == 0)
            .lazy_map(|x| x * x)
            .collect();
        assert_eq!(results, vec![4, 16, 36, 64, 100]);
    }

    #[test]
    fn test_lazy_iterator_with_take() {
        let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let results: Vec<i32> = numbers
            .into_lazy_iter()
            .lazy_filter(|&x| x > 3)
            .lazy_map(|x| x * x)
            .take(2)
            .collect();
        assert_eq!(results, vec![16, 25]);
    }

    #[test]
    fn test_lazy_iterator_for_each() {
        let numbers = vec![1, 2, 3, 4, 5];
        let mut results = Vec::new();

        numbers
            .into_lazy_iter()
            .lazy_filter(|&x| x % 2 == 1)
            .lazy_map(|x| x * 3)
            .for_each(|x| results.push(x));

        assert_eq!(results, vec![3, 9, 15]);
    }

    #[test]
    fn test_lazy_iterator_empty_collection() {
        let empty: Vec<i32> = vec![];
        let results: Vec<i32> = empty
            .into_lazy_iter()
            .lazy_map(|x| x * 2)
            .lazy_filter(|&x| x > 0)
            .collect();
        assert_eq!(results, Vec::<i32>::new());
    }

    #[test]
    fn test_lazy_iterator_as_regular_iterator() {
        let numbers = vec![1, 2, 3, 4, 5];
        let mut lazy_iter = numbers.into_lazy_iter();

        assert_eq!(lazy_iter.next(), Some(1));
        assert_eq!(lazy_iter.next(), Some(2));
        assert_eq!(lazy_iter.next(), Some(3));
    }

    #[test]
    fn test_lazy_iterator_filter_all_out() {
        let numbers = vec![1, 3, 5, 7, 9];
        let results: Vec<i32> = numbers
            .into_lazy_iter()
            .lazy_filter(|&x| x % 2 == 0)
            .collect();
        assert_eq!(results, Vec::<i32>::new());
    }

    #[test]
    fn test_lazy_iterator_complex_chain() {
        let numbers = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
        let results: Vec<i32> = numbers
            .into_lazy_iter()
            .lazy_map(|x| x + 1)
            .lazy_filter(|&x| x > 5)
            .lazy_map(|x| x * 2)
            .lazy_filter(|&x| x < 20)
            .collect();
        assert_eq!(results, vec![12, 14, 16, 18]);
    }
}
