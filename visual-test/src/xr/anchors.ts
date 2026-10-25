/**
 * Persistent object placement through the WebXR anchors API. Every placed object
 * is an XRAnchor; its persistent handle, kind and data live in localStorage under
 * `key`, so a page reload puts everything back where it was in the tracking
 * frame. On runtimes without anchors or persistent handles the pose itself is
 * stored instead. Cross-platform: the same calls work on Quest Browser.
 *
 * On this NorthStar the tracking frame is the XR50's SLAM frame, which lives
 * for as long as the bridge keeps the sensor running; a headset power cycle
 * starts a new frame (CSLAM map save/load would fix that, see WEBXR_PLAN.md).
 */
import * as THREE from 'three'

export interface Placed<D = unknown> {
  uuid: string
  kind: string
  data: D
  object: THREE.Object3D
  anchor: XRAnchor | null
  /** Set while the object is being moved by hand; the anchor pose is not applied then. */
  held: boolean
}

interface Stored<D> {
  kind: string
  data: D
  /** Fallback pose (local-floor space) when the runtime has no anchors or lost the handle */
  matrix: number[]
}

export interface Placements<D = unknown> {
  readonly items: Map<string, Placed<D>>
  /** Recreates every stored object; call once the session has a reference space. */
  restore: (session: XRSession, frame: XRFrame, space: XRReferenceSpace) => Promise<void>
  /** Anchors a new object at `matrix` (world = reference space). */
  place: (frame: XRFrame, space: XRReferenceSpace, kind: string, data: D, matrix: THREE.Matrix4) => Promise<Placed<D>>
  /** Re-anchors an object where it is now (after a grab). */
  move: (frame: XRFrame, space: XRReferenceSpace, placed: Placed<D>) => Promise<void>
  remove: (placed: Placed<D>) => void
  clear: () => void
  /** Per frame: drives every object from its anchor pose. */
  update: (frame: XRFrame, space: XRReferenceSpace) => void
}

type PersistentAnchor = XRAnchor & { requestPersistentHandle?: () => Promise<string> }
type PersistentSession = XRSession & { deletePersistentAnchor?: (uuid: string) => Promise<void> }

const position = new THREE.Vector3()
const quaternion = new THREE.Quaternion()
const scale = new THREE.Vector3()
const matrix = new THREE.Matrix4()

export const createPlacements = <D>(key: string, create: (kind: string, data: D) => THREE.Object3D, scene: THREE.Object3D): Placements<D> => {
  const items = new Map<string, Placed<D>>()
  const load = (): Record<string, Stored<D>> => JSON.parse(localStorage.getItem(key) ?? '{}')
  const save = (stored: Record<string, Stored<D>>) => localStorage.setItem(key, JSON.stringify(stored))
  const store = (placed: Placed<D>) => {
    const stored = load()
    stored[placed.uuid] = { kind: placed.kind, data: placed.data, matrix: placed.object.matrixWorld.toArray() }
    save(stored)
  }
  const forget = (uuid: string) => {
    const stored = load()
    delete stored[uuid]
    save(stored)
  }

  /** Anchor at `m` with a persistent handle where the runtime offers one. */
  const anchorAt = async (frame: XRFrame, space: XRReferenceSpace, m: THREE.Matrix4) => {
    if (!frame.createAnchor) return { anchor: null, uuid: crypto.randomUUID() }
    m.decompose(position, quaternion, scale)
    const anchor = (await frame.createAnchor(new XRRigidTransform(position, quaternion), space)) as PersistentAnchor
    const uuid = (await anchor.requestPersistentHandle?.()) ?? crypto.randomUUID()
    return { anchor, uuid }
  }

  const add = (uuid: string, kind: string, data: D, anchor: XRAnchor | null, m: THREE.Matrix4) => {
    const object = create(kind, data)
    m.decompose(object.position, object.quaternion, scale)
    scene.add(object)
    object.updateMatrixWorld(true)
    const placed: Placed<D> = { uuid, kind, data, object, anchor, held: false }
    items.set(uuid, placed)
    return placed
  }

  const dropAnchor = (placed: Placed<D>, session: PersistentSession | null) => {
    placed.anchor?.delete()
    void session?.deletePersistentAnchor?.(placed.uuid).catch(() => {})
    placed.anchor = null
  }

  return {
    items,
    restore: async (session, frame, space) => {
      const stored = load()
      // Every anchor request is issued now, while the frame is active; awaiting comes after.
      const pending = Object.entries(stored)
        .filter(([uuid]) => !items.has(uuid))
        .map(([uuid, entry]) => {
          matrix.fromArray(entry.matrix)
          const known = session.persistentAnchors?.includes(uuid) && session.restorePersistentAnchor
          const anchor = known
            ? session.restorePersistentAnchor!(uuid).catch(() => null)
            : frame.createAnchor
              ? anchorAt(frame, space, matrix).then((made) => made.anchor)
              : Promise.resolve(null)
          return { uuid, entry, matrix: matrix.clone(), anchor }
        })
      for (const { uuid, entry, matrix: m, anchor } of pending) add(uuid, entry.kind, entry.data, await anchor, m)
    },
    place: async (frame, space, kind, data, m) => {
      const { anchor, uuid } = await anchorAt(frame, space, m)
      const placed = add(uuid, kind, data, anchor, m)
      store(placed)
      return placed
    },
    move: async (frame, space, placed) => {
      dropAnchor(placed, frame.session as PersistentSession)
      items.delete(placed.uuid)
      forget(placed.uuid)
      const { anchor, uuid } = await anchorAt(frame, space, placed.object.matrixWorld)
      placed.anchor = anchor
      placed.uuid = uuid
      items.set(uuid, placed)
      store(placed)
    },
    remove: (placed) => {
      dropAnchor(placed, null)
      items.delete(placed.uuid)
      forget(placed.uuid)
      placed.object.removeFromParent()
    },
    clear: () => {
      for (const placed of [...items.values()]) {
        dropAnchor(placed, null)
        placed.object.removeFromParent()
      }
      items.clear()
      save({})
    },
    update: (frame, space) => {
      for (const placed of items.values()) {
        if (placed.held || !placed.anchor) continue
        const pose = frame.getPose(placed.anchor.anchorSpace, space)
        if (!pose) continue
        matrix.fromArray(pose.transform.matrix).decompose(placed.object.position, placed.object.quaternion, scale)
      }
    },
  }
}
