/*
 * Copyright 2023 ByteDance and/or its affiliates.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::pin::Pin;
use std::sync::Arc;

use anyhow::anyhow;
use openssl_async_job::{OpensslAsyncTask, SyncOperation, TokioAsyncOperation};

use super::KeylessOpensslArgs;

pub(super) struct KeylessOpensslAsyncJob {
    args: Arc<KeylessOpensslArgs>,
    result: Vec<u8>,
}

impl SyncOperation for KeylessOpensslAsyncJob {
    fn run(&mut self) -> anyhow::Result<()> {
        self.result = self.args.handle_action()?;
        Ok(())
    }
}

impl KeylessOpensslAsyncJob {
    pub(super) fn new(args: Arc<KeylessOpensslArgs>) -> Self {
        KeylessOpensslAsyncJob {
            args,
            result: Vec::new(),
        }
    }

    pub(super) async fn run(self) -> anyhow::Result<Vec<u8>> {
        let async_op = TokioAsyncOperation::new(self);
        let mut task = unsafe {
            OpensslAsyncTask::new(async_op)
                .map_err(|e| anyhow!("failed to create openssl async task: {e}"))?
        };
        Pin::new(&mut task).await.map_err(anyhow::Error::new)?;

        let sync_op = task.into_op().into_sync_op();
        Ok(sync_op.result)
    }
}
