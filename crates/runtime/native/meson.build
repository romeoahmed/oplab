# Built inside QEMU's per-target loop; upstream selects sources and dependencies.
export_options = {}
if host_os == 'darwin'
  exports = ['-Wl,-exported_symbol,_oplab_qemu']
elif host_os == 'windows'
  exports = []
  export_options = {'vs_module_defs': files('oplab/exports.def')[0]}
else
  exports = ['-Wl,--version-script=' + meson.current_source_dir() / 'oplab/exports.map']
  export_options = {'link_depends': files('oplab/exports.map')}
endif

shared_library('oplab-qemu-' + target_name,
  files('oplab/adapter.c', 'oplab/translate.c', 'oplab/memory.c', 'oplab/state.c'),
  link_args: exports,
  c_args: c_args,
  override_options: {'c_std': 'gnu23', 'werror': true},
  include_directories: target_inc,
  dependencies: arch_deps + [target_stubs],
  objects: lib.extract_all_objects(recursive: true),
  kwargs: export_options,
)
