interface BuiltinSampleEntryContext {
  itemCount: number
  search?: string
  type?: string
  progressMin?: number
  progressMax?: number
}

export function shouldOfferBuiltinSample({
  itemCount,
  search,
  type,
  progressMin,
  progressMax,
}: BuiltinSampleEntryContext): boolean {
  return (
    itemCount === 0 &&
    !search?.trim() &&
    type === undefined &&
    progressMin === undefined &&
    progressMax === undefined
  )
}
