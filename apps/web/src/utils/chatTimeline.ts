import type { Timelineitem, ToolCard } from '@/stores/viewModel'

export const MAX_TRANSCRIPT_ITEMS = 400

export type TimelineBlockItem =
  | { t: 'one'; key: string; item: Timelineitem }
  | { t: 'tools'; key: string; cards: ToolCard[] }

function itemId(item: Timelineitem): string {
  return 'id' in item ? item.id : item.callId
}

export function buildTimelineBlocks(items: readonly Timelineitem[]): TimelineBlockItem[] {
  const blocks: TimelineBlockItem[] = []
  for (const item of items) {
    if (item.kind === 'tool') {
      const last = blocks[blocks.length - 1]
      if (last?.t === 'tools') {
        last.cards.push(item)
      } else {
        blocks.push({ t: 'tools', key: `g:${item.callId}`, cards: [item] })
      }
      continue
    }
    blocks.push({ t: 'one', key: `${item.kind}:${itemId(item)}`, item })
  }
  return blocks
}

export function transcriptTail(
  items: readonly Timelineitem[],
  limit = MAX_TRANSCRIPT_ITEMS,
): Timelineitem[] {
  if (!Number.isInteger(limit) || limit <= 0) return []
  return items.slice(-limit)
}

export function parseTranscript(raw: string | null): Timelineitem[] {
  if (!raw) return []
  try {
    const parsed: unknown = JSON.parse(raw)
    return Array.isArray(parsed) ? parsed as Timelineitem[] : []
  } catch {
    return []
  }
}
