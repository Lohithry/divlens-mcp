//! Memory module - Vector-backed semantic memory (LanceDB + ONNX/fastembed)
//! Disabled by default via `vector-memory` feature to reduce bundle size.

#[cfg(feature = "vector-memory")]
use {
    std::sync::Mutex,
    arrow_array::{Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, StringArray},
    arrow_array::types::Float32Type,
    arrow_schema::{DataType, Field, Schema},
    fastembed::{EmbeddingModel, InitOptions, TextEmbedding},
    futures::stream::TryStreamExt,
    lancedb::query::{ExecutableQuery, QueryBase},
    lancedb::{connect, Connection},
};

pub struct MemoryManager {
    #[cfg(feature = "vector-memory")]
    db: Connection,
    #[cfg(feature = "vector-memory")]
    embedder: Mutex<TextEmbedding>,
    #[cfg(not(feature = "vector-memory"))]
    _phantom: std::marker::PhantomData<()>,
}

impl MemoryManager {
    pub async fn new(uri: &str) -> anyhow::Result<Self> {
        #[cfg(feature = "vector-memory")]
        {
            let db = connect(uri).execute().await?;
            let mut opts = InitOptions::new(EmbeddingModel::AllMiniLML6V2);
            opts.show_download_progress = true;
            let embedder = TextEmbedding::try_new(opts)?;
            Ok(Self {
                db,
                embedder: Mutex::new(embedder),
            })
        }

        #[cfg(not(feature = "vector-memory"))]
        {
            let _ = uri;
            Ok(Self {
                _phantom: std::marker::PhantomData,
            })
        }
    }

    pub async fn init_tables(&self) -> anyhow::Result<()> {
        #[cfg(feature = "vector-memory")]
        {
            let schema = std::sync::Arc::new(Schema::new(vec![
                Field::new("text", DataType::Utf8, false),
                Field::new(
                    "vector",
                    DataType::FixedSizeList(
                        std::sync::Arc::new(Field::new("item", DataType::Float32, true)),
                        384,
                    ),
                    false,
                ),
                Field::new("category", DataType::Utf8, false),
            ]));

            if !self.db.table_names().execute().await?.contains(&"memories".to_string()) {
                let empty_batch = RecordBatch::new_empty(schema.clone());
                let iter =
                    RecordBatchIterator::new(vec![Ok(empty_batch)].into_iter(), schema.clone());
                self.db.create_table("memories", Box::new(iter)).execute().await?;
            }
            Ok(())
        }

        #[cfg(not(feature = "vector-memory"))]
        {
            let _ = self;
            Ok(())
        }
    }

    pub async fn remember(&self, text: &str, category: &str) -> anyhow::Result<()> {
        #[cfg(feature = "vector-memory")]
        {
            let table = self.db.open_table("memories").execute().await?;
            let embeddings = self.embedder.lock().unwrap().embed(vec![text], None)?;
            let embedding_vec = &embeddings[0];

            let text_array = StringArray::from(vec![text]);
            let category_array = StringArray::from(vec![category]);
            let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
                vec![Some(embedding_vec.iter().copied().map(Some))],
                384,
            );

            let schema = table.schema().await?;
            let batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    std::sync::Arc::new(text_array),
                    std::sync::Arc::new(vector_array),
                    std::sync::Arc::new(category_array),
                ],
            )?;

            let iter = RecordBatchIterator::new(vec![Ok(batch)].into_iter(), schema.clone());
            table.add(Box::new(iter)).execute().await?;
            Ok(())
        }

        #[cfg(not(feature = "vector-memory"))]
        {
            let _ = (self, text, category);
            Ok(())
        }
    }

    pub async fn recall(&self, query: &str, limit: usize) -> anyhow::Result<Vec<String>> {
        #[cfg(feature = "vector-memory")]
        {
            let table = self.db.open_table("memories").execute().await?;
            let query_embedding = self.embedder.lock().unwrap().embed(vec![query], None)?;
            let query_vec = query_embedding[0].clone();

            let mut results = table
                .query()
                .nearest_to(query_vec)?
                .limit(limit)
                .execute()
                .await?;

            let mut found_memories = Vec::new();
            while let Some(batch) = results.try_next().await? {
                let text_col = batch.column_by_name("text").unwrap();
                let text_array = text_col.as_any().downcast_ref::<StringArray>().unwrap();
                for i in 0..text_array.len() {
                    found_memories.push(text_array.value(i).to_string());
                }
            }
            Ok(found_memories)
        }

        #[cfg(not(feature = "vector-memory"))]
        {
            let _ = (self, query, limit);
            Ok(Vec::new())
        }
    }
}
