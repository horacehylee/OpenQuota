import { describe, expect, it } from 'vitest';
import { projectSpend, spendBarSegments } from './totalSpend';
import type { UsageHistory } from './types';

const empty: UsageHistory = {
  today: null,
  yesterday: null,
  last30Days: null,
  daily: [],
  unknownModels: [],
};

function usage(today: UsageHistory['today']): UsageHistory {
  return { ...empty, today };
}

describe('Total Spend projection', () => {
  it('keeps token-only Codex data available without inventing cost data', () => {
    const providers = [
      {
        id: 'codex',
        usage: usage({
          tokens: 164_800_000,
          estimatedCostUsd: null,
          costEstimated: true,
          estimateComplete: false,
        }),
      },
      { id: 'antigravity', usage: empty },
    ];

    expect(projectSpend(providers, 'today', 'cost').slices).toEqual([]);
    expect(projectSpend(providers, 'today', 'tokens')).toMatchObject({
      centerValue: 164_800_000,
      slices: [{ id: 'codex', value: 164_800_000 }],
    });
  });

  it('does not let a token-only provider erase another provider cost', () => {
    const providers = [
      {
        id: 'claude',
        usage: usage({
          tokens: 1_000_000,
          estimatedCostUsd: 4,
          costEstimated: true,
          estimateComplete: true,
        }),
      },
      {
        id: 'codex',
        usage: usage({
          tokens: 9_000_000,
          estimatedCostUsd: null,
          costEstimated: true,
          estimateComplete: false,
        }),
      },
    ];

    expect(projectSpend(providers, 'today', 'cost')).toMatchObject({
      centerValue: 4,
      slices: [{ id: 'claude', value: 4 }],
    });
  });

  it('calculates a blended cost per million instead of summing provider rates', () => {
    const providers = [
      {
        id: 'claude',
        usage: usage({
          tokens: 1_000_000,
          estimatedCostUsd: 10,
          costEstimated: true,
          estimateComplete: true,
        }),
      },
      {
        id: 'codex',
        usage: usage({
          tokens: 3_000_000,
          estimatedCostUsd: 60,
          costEstimated: true,
          estimateComplete: true,
        }),
      },
    ];

    expect(projectSpend(providers, 'today', 'costPerMillion').centerValue).toBe(17.5);
  });

  it('tracks local estimation independently from pricing coverage', () => {
    const providers = [
      {
        id: 'claude',
        usage: usage({
          tokens: 1_000,
          estimatedCostUsd: 2,
          costEstimated: true,
          estimateComplete: false,
        }),
      },
    ];

    expect(projectSpend(providers, 'today', 'cost')).toMatchObject({
      costEstimated: true,
      estimateComplete: false,
    });
    expect(projectSpend(providers, 'today', 'tokens')).toMatchObject({
      costEstimated: false,
      estimateComplete: false,
    });
  });
});

describe('Total Spend bar segments', () => {
  const period = {
    tokens: 1_000_000,
    estimatedCostUsd: 10,
    costEstimated: true,
    estimateComplete: true,
  };

  it('shares the bar by value and keeps tiny providers visible', () => {
    const slices = [
      { id: 'codex', value: 99, period },
      { id: 'claude', value: 1, period },
    ];
    const shares = spendBarSegments(slices, 'cost');
    expect(shares).toEqual([
      { id: 'codex', share: expect.closeTo(0.975, 2) },
      { id: 'claude', share: expect.closeTo(0.025, 2) },
    ]);
    expect(spendBarSegments([slices[0]], 'cost')).toEqual([{ id: 'codex', share: 1 }]);
    expect(spendBarSegments([], 'cost')).toEqual([]);
  });

  it('weights cost-per-million segments by spend, not rate', () => {
    const shares = spendBarSegments(
      [
        { id: 'codex', value: 60, period: { ...period, estimatedCostUsd: 60 } },
        { id: 'claude', value: 20, period: { ...period, estimatedCostUsd: 10 } },
      ],
      'costPerMillion',
    );
    expect(shares).toEqual([
      { id: 'codex', share: expect.closeTo(6 / 7, 2) },
      { id: 'claude', share: expect.closeTo(1 / 7, 2) },
    ]);
    expect(
      spendBarSegments(
        [{ id: 'codex', value: 60, period: { ...period, estimatedCostUsd: null } }],
        'costPerMillion',
      ),
    ).toEqual([]);
  });
});
