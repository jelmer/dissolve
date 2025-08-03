use anyhow::Result;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::mypy_lsp::MypyTypeIntrospector;
use crate::pyright_lsp::PyrightLspClient;
use crate::types::TypeIntrospectionMethod;

/// Context that holds type introspection clients across multiple file migrations
pub struct TypeIntrospectionContext {
    method: TypeIntrospectionMethod,
    pyright_client: Option<Rc<RefCell<PyrightLspClient>>>,
    mypy_client: Option<Rc<RefCell<MypyTypeIntrospector>>>,
    file_versions: std::collections::HashMap<String, i32>,
    is_shutdown: bool,
}

impl TypeIntrospectionContext {
    /// Create a new type introspection context
    pub fn new(method: TypeIntrospectionMethod) -> Result<Self> {
        Self::new_with_workspace(method, None)
    }

    /// Create a new type introspection context with a specific workspace root
    pub fn new_with_workspace(
        method: TypeIntrospectionMethod,
        workspace_root: Option<&str>,
    ) -> Result<Self> {
        let (pyright_client, mypy_client) = match method {
            TypeIntrospectionMethod::PyrightLsp => {
                let client = PyrightLspClient::new(workspace_root)?;
                (Some(Rc::new(RefCell::new(client))), None)
            }
            TypeIntrospectionMethod::MypyDaemon => {
                let client = MypyTypeIntrospector::new(None)
                    .map_err(|e| anyhow::anyhow!("Failed to create mypy client: {}", e))?;
                (None, Some(Rc::new(RefCell::new(client))))
            }
            TypeIntrospectionMethod::PyrightWithMypyFallback => {
                let pyright = match PyrightLspClient::new(workspace_root) {
                    Ok(client) => Some(Rc::new(RefCell::new(client))),
                    Err(_) => None,
                };
                let mypy = match MypyTypeIntrospector::new(workspace_root) {
                    Ok(client) => Some(Rc::new(RefCell::new(client))),
                    Err(_) => None,
                };
                if pyright.is_none() && mypy.is_none() {
                    return Err(anyhow::anyhow!(
                        "Failed to initialize any type introspection client"
                    ));
                }
                (pyright, mypy)
            }
        };

        Ok(Self {
            method,
            pyright_client,
            mypy_client,
            file_versions: std::collections::HashMap::new(),
            is_shutdown: false,
        })
    }

    /// Get the type introspection method
    pub fn method(&self) -> TypeIntrospectionMethod {
        self.method
    }

    /// Get a clone of the pyright client if available
    pub fn pyright_client(&self) -> Option<Rc<RefCell<PyrightLspClient>>> {
        self.pyright_client.as_ref().map(|rc| rc.clone())
    }

    /// Get a clone of the mypy client if available
    pub fn mypy_client(&self) -> Option<Rc<RefCell<MypyTypeIntrospector>>> {
        self.mypy_client.as_ref().map(|rc| rc.clone())
    }

    /// Open a file for type introspection
    pub fn open_file(&mut self, file_path: &Path, content: &str) -> Result<()> {
        let path_str = file_path.to_string_lossy();
        self.file_versions.insert(path_str.to_string(), 1);

        if let Some(ref client) = self.pyright_client {
            client.borrow_mut().open_file(&path_str, content)?;
        }

        Ok(())
    }

    /// Update a file after modifications
    pub fn update_file(&mut self, file_path: &Path, content: &str) -> Result<()> {
        let path_str = file_path.to_string_lossy();
        let version = self
            .file_versions
            .get(path_str.as_ref())
            .copied()
            .unwrap_or(1)
            + 1;
        self.file_versions.insert(path_str.to_string(), version);

        if let Some(ref client) = self.pyright_client {
            client
                .borrow_mut()
                .update_file(&path_str, content, version)?;
        }

        if let Some(ref client) = self.mypy_client {
            client
                .borrow_mut()
                .invalidate_file(&path_str)
                .map_err(|e| anyhow::anyhow!("Failed to invalidate mypy cache: {}", e))?;
        }

        Ok(())
    }

    /// Check if the context has been shutdown
    pub fn is_shutdown(&self) -> bool {
        self.is_shutdown
    }

    /// Shutdown the clients cleanly
    pub fn shutdown(&mut self) -> Result<()> {
        if self.is_shutdown {
            return Ok(());
        }

        if let Some(ref client) = self.pyright_client {
            client.borrow_mut().shutdown()?;
        }

        if let Some(ref client) = self.mypy_client {
            client
                .borrow_mut()
                .stop_daemon()
                .map_err(|e| anyhow::anyhow!("Failed to stop mypy daemon: {}", e))?;
        }

        self.is_shutdown = true;
        Ok(())
    }
}

impl Drop for TypeIntrospectionContext {
    fn drop(&mut self) {
        // Try to shutdown cleanly, but don't panic on failure
        let _ = self.shutdown();
    }
}
