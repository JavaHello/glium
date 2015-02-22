use gl;
use std::{ffi, mem, ptr};
use std::sync::Arc;
use std::sync::mpsc::channel;
use {Display, DisplayImpl, GlObject};
use context::GlVersion;
use Handle;

use program::COMPILER_GLOBAL_LOCK;
use program::ProgramCreationError;

pub struct Shader {
    display: Arc<DisplayImpl>,
    id: Handle,
}

impl GlObject for Shader {
    type Id = Handle;

    fn get_id(&self) -> Handle {
        self.id
    }
}

impl Drop for Shader {
    fn drop(&mut self) {
        let id = self.id.clone();
        self.display.context.exec(move |ctxt| {
            unsafe {
                match id {
                    Handle::Id(id) => {
                        assert!(ctxt.version >= &GlVersion(2, 0));
                        ctxt.gl.DeleteShader(id);
                    },
                    Handle::Handle(id) => {
                        assert!(ctxt.extensions.gl_arb_shader_objects);
                        ctxt.gl.DeleteObjectARB(id);
                    }
                }
            }
        });
    }
}

/// Builds an individual shader.
pub fn build_shader(display: &Display, shader_type: gl::types::GLenum, source_code: &str)
                    -> Result<Shader, ProgramCreationError>
{
    let source_code = ffi::CString::from_slice(source_code.as_bytes());

    let (tx, rx) = channel();
    display.context.context.exec(move |ctxt| {
        unsafe {
            if shader_type == gl::GEOMETRY_SHADER && ctxt.opengl_es {
                tx.send(Err(ProgramCreationError::ShaderTypeNotSupported)).ok();
                return;
            }

            let id = if ctxt.version >= &GlVersion(2, 0) {
                Handle::Id(ctxt.gl.CreateShader(shader_type))
            } else if ctxt.extensions.gl_arb_shader_objects {
                Handle::Handle(ctxt.gl.CreateShaderObjectARB(shader_type))
            } else {
                unreachable!()
            };

            if id == Handle::Id(0) || id == Handle::Handle(0 as gl::types::GLhandleARB) {
                tx.send(Err(ProgramCreationError::ShaderTypeNotSupported)).ok();
                return;
            }

            match id {
                Handle::Id(id) => {
                    assert!(ctxt.version >= &GlVersion(2, 0));
                    ctxt.gl.ShaderSource(id, 1, [ source_code.as_ptr() ].as_ptr(), ptr::null());
                },
                Handle::Handle(id) => {
                    assert!(ctxt.extensions.gl_arb_shader_objects);
                    ctxt.gl.ShaderSourceARB(id, 1, [ source_code.as_ptr() ].as_ptr(), ptr::null());
                }
            }

            // compiling
            {
                let _lock = COMPILER_GLOBAL_LOCK.lock();

                match id {
                    Handle::Id(id) => {
                        assert!(ctxt.version >= &GlVersion(2, 0));
                        ctxt.gl.CompileShader(id);
                    },
                    Handle::Handle(id) => {
                        assert!(ctxt.extensions.gl_arb_shader_objects);
                        ctxt.gl.CompileShaderARB(id);
                    }
                }
            }

            // checking compilation success
            let compilation_success = {
                let mut compilation_success: gl::types::GLint = mem::uninitialized();
                match id {
                    Handle::Id(id) => {
                        assert!(ctxt.version >= &GlVersion(2, 0));
                        ctxt.gl.GetShaderiv(id, gl::COMPILE_STATUS, &mut compilation_success);
                    },
                    Handle::Handle(id) => {
                        assert!(ctxt.extensions.gl_arb_shader_objects);
                        ctxt.gl.GetObjectParameterivARB(id, gl::OBJECT_COMPILE_STATUS_ARB,
                                                        &mut compilation_success);
                    }
                }
                compilation_success
            };

            if compilation_success == 0 {
                // compilation error
                let mut error_log_size: gl::types::GLint = mem::uninitialized();

                match id {
                    Handle::Id(id) => {
                        assert!(ctxt.version >= &GlVersion(2, 0));
                        ctxt.gl.GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut error_log_size);
                    },
                    Handle::Handle(id) => {
                        assert!(ctxt.extensions.gl_arb_shader_objects);
                        ctxt.gl.GetObjectParameterivARB(id, gl::OBJECT_INFO_LOG_LENGTH_ARB,
                                                        &mut error_log_size);
                    }
                }

                let mut error_log: Vec<u8> = Vec::with_capacity(error_log_size as usize);

                match id {
                    Handle::Id(id) => {
                        assert!(ctxt.version >= &GlVersion(2, 0));
                        ctxt.gl.GetShaderInfoLog(id, error_log_size, &mut error_log_size,
                                                 error_log.as_mut_slice().as_mut_ptr()
                                                   as *mut gl::types::GLchar);
                    },
                    Handle::Handle(id) => {
                        assert!(ctxt.extensions.gl_arb_shader_objects);
                        ctxt.gl.GetInfoLogARB(id, error_log_size, &mut error_log_size,
                                              error_log.as_mut_slice().as_mut_ptr()
                                                as *mut gl::types::GLchar);
                    }
                }

                error_log.set_len(error_log_size as usize);

                let msg = String::from_utf8(error_log).unwrap();
                tx.send(Err(ProgramCreationError::CompilationError(msg))).ok();
                return;
            }

            tx.send(Ok(id)).unwrap();
        }
    });

    rx.recv().unwrap().map(|id| {
        Shader {
            display: display.context.clone(),
            id: id
        }
    })
}
