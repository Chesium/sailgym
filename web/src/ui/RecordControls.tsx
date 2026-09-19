import { useEffect, useRef, useState } from 'react'

import {
  binaryBlob,
  download,
  episodeFilename,
  importEpisode,
  jsonBlob,
} from '../sim/episodeIo'
import type { SimHandle } from '../sim/loadWasm'
import type { Episode } from '../sim/scenarioTypes'

/**
 * Start/stop recording, download the episode, and load one back (brief §33,
 * task 9.5).
 *
 * The recorder lives in Rust and is an observer: starting one cannot change
 * a trajectory (`recording::tests::recording_does_not_perturb`). This
 * component only calls `start_recording` / `stop_recording`, hands the
 * resulting document to the two core codecs, and reports failures.
 *
 * Both serialisations are the core's (brief §33) and so is the
 * schema-version check, so an episode from another build is refused with the
 * core's message rather than crashing the page.
 */

/** Logging rates offered, Hz of **simulated** time (brief §33). */
export const LOG_RATES = [5, 10, 20, 50] as const
export type LogRate = (typeof LOG_RATES)[number]

/** How often the frame counter is re-read while recording, ms. */
const COUNTER_INTERVAL_MS = 250

export interface RecordControlsProps {
  ready: boolean
  withSim: <T>(fn: (sim: SimHandle) => T) => T | null
  episode: Episode | null
  onEpisode: (episode: Episode | null) => void
  /** Enter replay of the episode currently held. */
  onReplay: () => void
  replaying: boolean
}

export function RecordControls({
  ready,
  withSim,
  episode,
  onEpisode,
  onReplay,
  replaying,
}: RecordControlsProps) {
  const [recording, setRecording] = useState(false)
  const [frames, setFrames] = useState(0)
  const [logHz, setLogHz] = useState<LogRate>(20)
  const [error, setError] = useState<string | null>(null)
  const fileRef = useRef<HTMLInputElement | null>(null)

  // The frame counter is the core's own, read at 4 Hz. It is not derived
  // from the clock: the logging rate is in *simulated* seconds, so at 4x the
  // count rises four times as fast and the readout has to show that.
  useEffect(() => {
    if (!recording) {
      return
    }
    const id = window.setInterval(() => {
      setFrames(withSim((sim) => sim.recorded_frames()) ?? 0)
    }, COUNTER_INTERVAL_MS)
    return () => window.clearInterval(id)
  }, [recording, withSim])

  // A reset discards an in-progress recording in the core (its header
  // describes a run that no longer exists), so the button must not go on
  // claiming to be recording.
  useEffect(() => {
    if (!recording) {
      return
    }
    const id = window.setInterval(() => {
      if (withSim((sim) => sim.is_recording()) === false) {
        setRecording(false)
      }
    }, COUNTER_INTERVAL_MS)
    return () => window.clearInterval(id)
  }, [recording, withSim])

  const toggle = () => {
    setError(null)
    if (!recording) {
      withSim((sim) => sim.start_recording(logHz))
      setFrames(withSim((sim) => sim.recorded_frames()) ?? 0)
      setRecording(true)
      return
    }
    setRecording(false)
    try {
      const json = withSim((sim) => sim.stop_recording() as string)
      if (json === null) {
        return
      }
      const parsed = JSON.parse(json) as Episode
      setFrames(parsed.frames.length)
      onEpisode(parsed)
    } catch (cause: unknown) {
      setError(typeof cause === 'string' ? cause : String(cause))
    }
  }

  const save = (format: 'json' | 'bin') => {
    if (episode === null) {
      return
    }
    setError(null)
    try {
      const json = JSON.stringify(episode)
      const blob =
        format === 'json'
          ? jsonBlob(json)
          : (withSim((sim) => binaryBlob(sim, json)) as Blob | null)
      if (blob === null) {
        return
      }
      download(blob, episodeFilename(episode, format))
    } catch (cause: unknown) {
      setError(typeof cause === 'string' ? cause : String(cause))
    }
  }

  const load = async (file: File) => {
    setError(null)
    try {
      const bytes = new Uint8Array(await file.arrayBuffer())
      const loaded = withSim((sim) => importEpisode(sim, bytes))
      if (loaded === null) {
        return
      }
      onEpisode(loaded)
      setFrames(loaded.frames.length)
    } catch (cause: unknown) {
      setError(cause instanceof Error ? cause.message : String(cause))
    }
  }

  return (
    <div
      data-testid="record-controls"
      data-playback={replaying ? 'replay' : 'live'}
      data-recording={recording ? 'true' : 'false'}
      data-frames={frames}
      data-log-hz={logHz}
      data-has-episode={episode === null ? 'false' : 'true'}
      style={{ display: 'flex', gap: 6, alignItems: 'center', flexWrap: 'wrap' }}
    >
      <button
        type="button"
        data-testid="record-toggle"
        disabled={!ready}
        onClick={toggle}
        style={{ fontWeight: recording ? 700 : 400 }}
      >
        {recording ? `■ Stop (${frames})` : '● Record'}
      </button>
      <label>
        at{' '}
        <select
          data-testid="record-hz"
          value={logHz}
          disabled={recording}
          onChange={(e) => setLogHz(Number(e.target.value) as LogRate)}
        >
          {LOG_RATES.map((hz) => (
            <option key={hz} value={hz}>
              {hz} Hz
            </option>
          ))}
        </select>
      </label>
      <button
        type="button"
        data-testid="record-replay"
        disabled={episode === null || replaying}
        onClick={onReplay}
      >
        Replay
      </button>
      <button
        type="button"
        data-testid="record-export-json"
        disabled={episode === null}
        onClick={() => save('json')}
      >
        Save JSON
      </button>
      <button
        type="button"
        data-testid="record-export-binary"
        disabled={episode === null}
        onClick={() => save('bin')}
      >
        Save binary
      </button>
      <button
        type="button"
        data-testid="record-import"
        disabled={!ready}
        onClick={() => fileRef.current?.click()}
      >
        Load episode…
      </button>
      <input
        ref={fileRef}
        type="file"
        data-testid="record-file"
        accept=".json,.bin,application/json,application/octet-stream"
        style={{ display: 'none' }}
        onChange={(e) => {
          const file = e.target.files?.[0]
          // Clear the value so re-choosing the same file fires `change`.
          e.target.value = ''
          if (file !== undefined) {
            void load(file)
          }
        }}
      />
      {episode !== null && (
        <span data-testid="record-summary" data-frame-count={episode.frames.length}>
          {episode.frames.length} frames · {episode.header.scenario.name}
        </span>
      )}
      {error !== null && (
        <span data-testid="record-error" style={{ color: '#a00' }}>
          {error}
        </span>
      )}
    </div>
  )
}
