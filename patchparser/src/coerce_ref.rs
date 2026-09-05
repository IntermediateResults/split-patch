pub trait CoerceRef {
    type U;
    fn coerce_ref(self) -> Self::U;
}

impl<'t, T, E> CoerceRef for Result<&'t mut T, E> {
    type U = Result<&'t T, E>;

    fn coerce_ref(self) -> Self::U {
        self.map(|v| -> &T { v })
    }
}

impl<'t, T, const N: usize> CoerceRef for [&'t mut T; N] {
    type U = [&'t T; N];

    fn coerce_ref(self) -> Self::U {
        self.map(|v| -> &T { v })
    }
}

impl<'t, T> CoerceRef for &'t mut T {
    type U = &'t T;

    fn coerce_ref(self) -> Self::U {
        self
    }
}
