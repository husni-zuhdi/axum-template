use moka::future::Cache;

pub async fn get<V>(cache: &Cache<String, V>, key: &str) -> Option<V>
where
    V: Clone + Send + Sync + 'static,
{
    cache.get(key).await
}

pub async fn put<V>(cache: &Cache<String, V>, key: String, value: V)
where
    V: Clone + Send + Sync + 'static,
{
    cache.insert(key, value).await;
}

pub async fn remove_pattern<V>(cache: &Cache<String, V>, prefix: &str)
where
    V: Clone + Send + Sync + 'static,
{
    let keys: Vec<String> = cache
        .iter()
        .filter(|(k, _)| k.starts_with(prefix))
        .map(|(k, _)| k.to_string())
        .collect();
    for key in keys {
        cache.invalidate(&key).await;
    }
}
