/**
 * racingedge C API - headless host entry points.
 *
 * Port of ksengine `include/ksengine_c.h`. An external host (a dedicated
 * server, a GDExtension, a test harness) drives a fixed-step vehicle plus a
 * session authority without linking Rust. Every handle-taking entry point is
 * null-safe: a null engine is a no-op, or produces a neutral value where a
 * result must be returned. The ABI is intended to stay stable across minor
 * versions.
 *
 * This header is hand-written; `src/c_api.rs` (`EXPORTED_SYMBOLS`) is the
 * source of truth and a unit test fails the build if the two drift apart.
 */
#ifndef RACINGEDGE_H
#define RACINGEDGE_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>

#if defined(_WIN32) && defined(RACINGEDGE_DLL)
#  ifdef RACINGEDGE_BUILD
#    define REAPI __declspec(dllexport)
#  else
#    define REAPI __declspec(dllimport)
#  endif
#else
#  define REAPI
#endif

typedef struct ReEngine ReEngine;

typedef struct ReVehicleState {
    float x, y, z;
    float yaw, pitch, roll;
    float speed_ms;
    float rpm;
    int32_t gear;
    float throttle, brake, steer;
    float fuel_l;
    float tyre_temp_fl, tyre_temp_fr, tyre_temp_rl, tyre_temp_rr;
} ReVehicleState;

typedef struct ReInput {
    float throttle; /* 0..1 */
    float brake;
    float steer;    /* -1..1 */
    float clutch;
    int32_t gear_request; /* 0 = none */
} ReInput;

/* ==========================================================================
 * Engine lifecycle.
 * ========================================================================== */

/** Create an engine instance. seed is reserved for deterministic runs; the
 *  current implementation is already deterministic. */
REAPI ReEngine* re_engine_create(uint32_t seed);

REAPI void re_engine_destroy(ReEngine* eng);

/** Fixed physics step in seconds (default 0.001). Non-positive values are
 *  ignored. */
REAPI void re_engine_set_timestep(ReEngine* eng, double dt);

/** Reset the vehicle and session to their initial state, keeping timestep and
 *  configured session parameters. */
REAPI void re_engine_reset(ReEngine* eng);

/** Apply driver input for subsequent steps. */
REAPI void re_engine_set_input(ReEngine* eng, const ReInput* in);

/** Advance the simulation by dt seconds, subdividing into fixed steps. */
REAPI void re_engine_step(ReEngine* eng, double dt);

/** Read the primary vehicle state. A null out is a no-op. */
REAPI void re_engine_get_state(const ReEngine* eng, ReVehicleState* out);

/** Optional: enable binary replay recording to path_utf8 (empty = stop).
 *  Returns non-zero on success; 0 means unsupported. */
REAPI int re_engine_set_replay_path(ReEngine* eng, const char* path_utf8);

/** Version string "major.minor.patch", static for the process lifetime. */
REAPI const char* re_engine_version(void);

/* ==========================================================================
 * Session control (dedicated server / headless race authority).
 * ========================================================================== */

typedef struct ReSessionConfig {
    int32_t total_laps;
    int32_t time_limit_s;
    float   countdown_s;
    float   track_length_m;
} ReSessionConfig;

typedef enum ReSessionPhase {
    RE_PHASE_IDLE = 0,
    RE_PHASE_COUNTDOWN = 1,
    RE_PHASE_GREEN = 2,
    RE_PHASE_CHECKERED = 3
} ReSessionPhase;

/** Configure the session. Call before re_engine_session_start. */
REAPI void re_engine_session_configure(ReEngine* eng, const ReSessionConfig* cfg);

/** Start the configured session (enters the countdown). */
REAPI void re_engine_session_start(ReEngine* eng);

REAPI ReSessionPhase re_engine_session_phase(const ReEngine* eng);

/** Current lap (1-based) while green; 0 before the race starts. */
REAPI int32_t re_engine_session_lap(const ReEngine* eng);

/** Seconds elapsed since the session started. */
REAPI double re_engine_session_time(const ReEngine* eng);

/** Seconds left on the time limit, or -1 when the session has none. */
REAPI double re_engine_session_remaining(const ReEngine* eng);

#ifdef __cplusplus
}
#endif

#endif /* RACINGEDGE_H */
