use iced::futures;

pub fn every(duration: std::time::Duration) -> iced::Subscription<chrono::DateTime<chrono::Local>> {
    iced::Subscription::from_recipe(Every(duration))
}

struct Every(std::time::Duration);

impl<H, I> iced_futures::subscription::Recipe<H, I> for Every
where
    H: std::hash::Hasher,
{
    type Output = chrono::DateTime<chrono::Local>;

    fn hash(&self, state: &mut H) {
        use std::hash::Hash;

        std::any::TypeId::of::<Self>().hash(state);
        self.0.hash(state);
    }

    fn stream(
        self: Box<Self>,
        _input: futures::stream::BoxStream<'static, I>,
    ) -> futures::stream::BoxStream<'static, Self::Output> {
        use futures::stream::StreamExt;

        async_std::stream::interval(self.0)
            .map(|_| chrono::Local::now())
            .boxed()
    }
}
