#include "qemu/osdep.h"

#include "internal.h"

#include "cpu.h"
#include "tcg/tcg.h"

#include "accel/accel-cpu-ops.h"
#include "accel/tcg/cpu-ops.h"
#include "accel/tcg/tcg-accel-ops.h"
#include "hw/core/cpu.h"
#include "hw/core/qdev.h"
#include "qapi/error.h"
#include "qemu-version.h"
#include "qemu/guest-random.h"
#include "qemu/main-loop.h"
#include "qemu/rcu.h"
#include "qemu/thread.h"
#include "qemu/units.h"
#include "system/cpus.h"
#include "system/replay.h"
#include "system/system.h"
#include "tcg/startup.h"

int (*qemu_main)(void);
static GMutex lock;
static TCGContext *context;
static bool initialized;

static void *cpu_thread(void *argument) {
  CPUState *cpu = argument;
  rcu_register_thread();
  bql_lock();
  qemu_thread_get_self(cpu->thread);
  cpu->thread_id = qemu_get_thread_id();
  current_cpu = cpu;
  cpu->neg.can_do_io = true;
  cpu_thread_signal_created(cpu);
  qemu_guest_random_seed_thread_part2(cpu->random_seed);
  do {
    qemu_process_cpu_events(cpu);
  } while (!cpu->unplug);
  cpu_thread_signal_destroyed(cpu);
  current_cpu = NULL;
  bql_unlock();
  rcu_unregister_thread();
  return NULL;
}

static void attach_cpu(CPUState *cpu) {
  tcg_cpu_init_cflags(cpu, false);
  qemu_thread_create(cpu->thread, "oplab-vcpu", cpu_thread, cpu, QEMU_THREAD_JOINABLE);
}

static void owner_init(Object *object) {
  OplabCpu *owner = OPLAB_CPU(object);
  owner->maps = g_ptr_array_new_with_free_func(g_free);
  memory_region_init(&owner->memory, object, "oplab-address-space", UINT64_MAX);
}

static void owner_finalize(Object *object) {
  OplabCpu *owner = OPLAB_CPU(object);
  g_ptr_array_free(owner->maps, true);
}

static const TypeInfo types[] = {{
    .name = TYPE_OPLAB_CPU,
    .parent = TYPE_OBJECT,
    .instance_size = sizeof(OplabCpu),
    .instance_init = owner_init,
    .instance_finalize = owner_finalize,
}};
DEFINE_TYPES(types)

static void initialize(void) {
  const char *options[] = {"oplab", "-machine", "none", "-accel",      "tcg,thread=single",
                           "-S",    "-display", "none", "-nodefaults", "-monitor",
                           "none",  "-serial",  "none", NULL};
  g_auto(GStrv) arguments = g_new0(char *, G_N_ELEMENTS(options));
  for (size_t i = 0; options[i]; ++i) {
    arguments[i] = g_strdup(options[i]);
  }
  qemu_init(G_N_ELEMENTS(options) - 1, arguments);
  static AccelOpsClass operations;
  operations = *cpus_get_accel();
  operations.create_vcpu_thread = attach_cpu;
  operations.kick_vcpu_thread = tcg_kick_vcpu_thread;
  cpus_register_accel(&operations);
  bql_unlock();
  replay_mutex_unlock();
  initialized = true;
}

void oplab_enter(void) {
  g_mutex_lock(&lock);
  rcu_register_thread();
  if (!initialized) {
    initialize();
  }
  bql_lock();
  if (context) {
    tcg_ctx = context;
  }
  current_cpu = NULL;
}

void oplab_leave(void) {
  current_cpu = NULL;
  bql_unlock();
  rcu_unregister_thread();
  g_mutex_unlock(&lock);
}

static OplabCpu *create(void) {
  oplab_enter();
  OplabCpu *owner = OPLAB_CPU(object_new(TYPE_OPLAB_CPU));
  object_property_add_child(machine_get_container("unattached"), "oplab-runtime[*]", OBJECT(owner));
#if defined(TARGET_X86_64)
  owner->cpu = CPU(object_new(X86_CPU_TYPE_NAME("max")));
  /* Independent user-mode experiments have no interrupt controller or SMP. */
  object_property_set_uint(OBJECT(owner->cpu), "apic-id", 0, &error_abort);
  object_property_set_bool(OBJECT(owner->cpu), "apic", false, &error_abort);
  object_property_set_bool(OBJECT(owner->cpu), "x2apic", false, &error_abort);
#else
  owner->cpu = CPU(object_new(ARM_CPU_TYPE_NAME("max")));
#endif
  owner->cpu->opaque = owner;
  object_property_set_link(OBJECT(owner->cpu), "memory", OBJECT(&owner->memory), &error_abort);
#if defined(TARGET_AARCH64)
  /* Set the strong QOM link explicitly so both address spaces own a reference. */
  object_property_set_link(OBJECT(owner->cpu), "secure-memory", OBJECT(&owner->memory),
                           &error_abort);
#endif
  object_property_add_child(OBJECT(owner), "cpu", OBJECT(owner->cpu));
  qdev_realize_and_unref(DEVICE(owner->cpu), NULL, &error_abort);
  static TCGCPUOps operations;
  operations = *owner->cpu->cc->tcg_ops;
  operations.tlb_fill_align = oplab_tlb_fill;
  CPU_GET_CLASS(owner->cpu)->tcg_ops = &operations;
  if (!context) {
    tcg_register_thread();
    context = tcg_ctx;
  }
  oplab_state_init(owner->cpu);
  oplab_leave();
  return owner;
}

static void destroy(OplabCpu *owner) {
  oplab_enter();
#if defined(TARGET_X86_64)
  /* TCG registers this during realization; CPU unplug does not remove it. */
  qemu_remove_machine_init_done_notifier(&X86_CPU(owner->cpu)->machine_done);
#endif
#if defined(TARGET_AARCH64)
  cpu_remove_sync(owner->cpu);
#endif
  qdev_unrealize(DEVICE(owner->cpu));
  /* CPU finalization may be deferred by RCU-owned address spaces. Release
   * links to embedded memory while its owner is still retained here. */
  object_property_set_link(OBJECT(owner->cpu), "memory", NULL, &error_abort);
#if defined(TARGET_AARCH64)
  object_property_set_link(OBJECT(owner->cpu), "secure-memory", NULL, &error_abort);
#endif
  object_unparent(OBJECT(owner->cpu));
  for (unsigned i = 0; i < owner->maps->len; ++i) {
    OplabMap *map = g_ptr_array_index(owner->maps, i);
    memory_region_del_subregion(&owner->memory, &map->memory);
  }
  object_unparent(OBJECT(owner));
  object_unref(OBJECT(owner));
  oplab_leave();
}

static bool map_memory(OplabCpu *owner, uint64_t base, uint64_t size, uint32_t permissions,
                       const uint8_t *bytes, size_t length) {
  if (!size || size > 64 * MiB || base > UINT64_MAX - (size - 1) || base % 4096 || size % 4096 ||
      length > size || permissions > 7) {
    return false;
  }
  oplab_enter();
  bool valid = owner->maps->len < 64 && size <= 64 * MiB - owner->mapped;
  for (unsigned i = 0; valid && i < owner->maps->len; ++i) {
    OplabMap *map = g_ptr_array_index(owner->maps, i);
    valid = base >= map->base ? base - map->base >= map->size : map->base - base >= size;
  }
  if (valid) {
    OplabMap *map = g_new0(OplabMap, 1);
    map->base = base;
    map->size = size;
    map->permissions = permissions;
    Error *error = NULL;
    g_autofree char *name = g_strdup_printf("oplab-memory-%u", owner->maps->len);
    /* Sandbox RAM has QOM ownership but does not participate in VM migration. */
    memory_region_init_ram_flags_nomigrate(&map->memory, OBJECT(owner), name, size, 0, &error);
    if (error) {
      error_free(error);
      object_unparent(OBJECT(&map->memory));
      g_free(map);
      valid = false;
    } else {
      if (length) {
        memcpy(memory_region_get_ram_ptr(&map->memory), bytes, length);
      }
      memory_region_add_subregion(&owner->memory, base, &map->memory);
      g_ptr_array_add(owner->maps, map);
      owner->mapped += size;
    }
  }
  oplab_leave();
  return valid;
}

static bool read_memory(OplabCpu *owner, uint64_t address, uint8_t *bytes, size_t size) {
  if (!size || size > 64 * KiB) {
    return false;
  }
  oplab_enter();
  OplabMap *map = oplab_mapping(owner, address, size);
  if (map) {
    memcpy(bytes, memory_region_get_ram_ptr(&map->memory) + address - map->base, size);
  }
  oplab_leave();
  return map != NULL;
}

static bool write_memory(OplabCpu *owner, uint64_t address, const uint8_t *bytes, size_t size) {
  if (!size || size > 64 * KiB) {
    return false;
  }
  oplab_enter();
  OplabMap *map = oplab_mapping(owner, address, size);
  if (map) {
    memcpy(memory_region_get_ram_ptr(&map->memory) + address - map->base, bytes, size);
  }
  oplab_leave();
  return map != NULL;
}

static bool read_register(OplabCpu *owner, OplabRegister bank, uint32_t index, uint8_t *bytes,
                          size_t size) {
  oplab_enter();
  bool result = oplab_state_read(owner, bank, index, bytes, size);
  oplab_leave();
  return result;
}
static bool write_register(OplabCpu *owner, OplabRegister bank, uint32_t index,
                           const uint8_t *bytes, size_t size) {
  oplab_enter();
  bool result = oplab_state_write(owner, bank, index, bytes, size);
  oplab_leave();
  return result;
}

const OplabQemu *oplab_qemu(void) {
  /* The loader calls this on the dlopen thread. Detach QEMU's constructor
   * registration; public operations use call-scoped RCU registration so
   * host TLS destructor order cannot leave a dangling registry entry. */
  static gsize detached;
  if (g_once_init_enter(&detached)) {
    rcu_unregister_thread();
    g_once_init_leave(&detached, 1);
  }
  static const OplabQemu api = {
      .abi = 1,
      .version = QEMU_VERSION,
      .create = create,
      .destroy = destroy,
      .map = map_memory,
      .read = read_memory,
      .write = write_memory,
      .register_read = read_register,
      .register_write = write_register,
      .translate = oplab_translate,
      .release = oplab_block_free,
      .execute = oplab_execute,
      .load = (uintptr_t)oplab_load,
      .store = (uintptr_t)oplab_store,
      .load_pair = (uintptr_t)oplab_load_pair,
      .store_pair = (uintptr_t)oplab_store_pair,
  };
  return &api;
}
