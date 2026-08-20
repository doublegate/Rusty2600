// Quick manual trace: write TIM1T=5, tick once (the earliest a program can
// observe a read, since a write and the next read are always separated by
// at least one CPU cycle), check INTIM.
