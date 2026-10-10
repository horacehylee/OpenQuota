<script lang="ts">
  import { onDestroy } from 'svelte';
  import Icon from './Icon.svelte';
  import {
    formatMetricValue,
    formatSpendReading,
    formatSpendValue,
    totalSpendRingCenter,
  } from './metricFormat';
  import SelectMenu from './SelectMenu.svelte';
  import type { ProviderCatalogIndex } from './metrics';
  import { TOTAL_SPEND_GEOMETRY } from './shareCard';
  import {
    emptySpendMessage,
    projectSpend,
    spendBarSegments,
    type SpendProjection,
  } from './totalSpend';
  import { providerFamily } from './providerIconPaths';
  import type { AppSettings, UsageHistory } from './types';

  interface Props {
    providers: Array<{ id: string; usage: UsageHistory }>;
    settings: AppSettings;
    catalog: ProviderCatalogIndex;
    onChange: (settings: AppSettings) => void;
    onShare: (projection: SpendProjection) => boolean | Promise<boolean>;
  }
  let { providers, settings, catalog, onChange, onShare }: Props = $props();
  const providerDisplayName = (id: string) => catalog.displayName(id, settings.providerNames);
  const projection = $derived(
    projectSpend(providers, settings.totalSpendPeriod, settings.totalSpendMetric),
  );
  const providerNames = $derived(providers.map((provider) => providerDisplayName(provider.id)));
  const barSegments = $derived(spendBarSegments(projection.slices, settings.totalSpendMetric));
  const periodIndex = $derived(
    settings.totalSpendPeriod === 'today' ? 0 : settings.totalSpendPeriod === 'yesterday' ? 1 : 2,
  );
  let shareCopied = $state(false);
  let shareTimer: ReturnType<typeof setTimeout> | undefined;

  onDestroy(() => {
    if (shareTimer) clearTimeout(shareTimer);
  });

  function display(value: number | null, tokens: number) {
    if (value === null) return '—';
    if (settings.totalSpendMetric === 'cost') return formatSpendReading(value, tokens);
    return formatSpendValue(value, settings.totalSpendMetric);
  }
  function totalParts(value: number | null) {
    if (value === null) return { primary: '—', unit: '' };
    return totalSpendRingCenter(value, settings.totalSpendMetric);
  }
  function segmentTitle(id: string) {
    const slice = projection.slices.find((item) => item.id === id);
    if (!slice) return undefined;
    const value = formatSpendValue(slice.value, settings.totalSpendMetric, 'full');
    if (settings.totalSpendMetric !== 'cost') return `${providerDisplayName(id)}: ${value}`;
    const tokens = formatMetricValue(slice.period.tokens, 'count', 'full', 'tokens');
    return `${providerDisplayName(id)}: ${value} · ${tokens}`;
  }
  function centerTooltip(value: number | null) {
    if (value === null) return undefined;
    const exact = formatSpendValue(value, settings.totalSpendMetric, 'full');
    if (projection.costEstimated && settings.totalSpendMetric !== 'tokens') {
      return `${exact} · Estimated locally, so it may be off`;
    }
    return exact;
  }
  function metricTitle() {
    if (settings.totalSpendMetric === 'tokens') return 'Tokens';
    if (settings.totalSpendMetric === 'costPerMillion') return 'Cost/MTok';
    return 'Cost';
  }
  function patch(patch: Partial<AppSettings>) {
    onChange({ ...settings, ...patch });
  }
  async function share() {
    if (!(await onShare(projection))) return;
    shareCopied = true;
    if (shareTimer) clearTimeout(shareTimer);
    shareTimer = setTimeout(() => (shareCopied = false), 1400);
  }
</script>

<section
  class="total-spend-section"
  aria-label="Total Spend"
  data-total-spend
  style={`--total-card-padding-x:${TOTAL_SPEND_GEOMETRY.cardPaddingX}px;--total-card-padding-y:${TOTAL_SPEND_GEOMETRY.cardPaddingY}px;--total-switcher-height:${TOTAL_SPEND_GEOMETRY.switcherHeight}px;--total-period-size:${TOTAL_SPEND_GEOMETRY.periodFontSize}px;--total-body-gap:${TOTAL_SPEND_GEOMETRY.bodyGap}px;--total-legend-gap:${TOTAL_SPEND_GEOMETRY.legendGap}px;--total-bar-height:${TOTAL_SPEND_GEOMETRY.barHeight}px;--total-line-height:${TOTAL_SPEND_GEOMETRY.totalLineHeight}px;--total-value-size:${TOTAL_SPEND_GEOMETRY.totalFontSize}px;--total-value-unit-size:${TOTAL_SPEND_GEOMETRY.totalUnitFontSize}px;--total-legend-size:${TOTAL_SPEND_GEOMETRY.legendFontSize}px;`}
>
  <div class="total-card__header">
    <div class="total-card__title">
      <SelectMenu
        label="Total Spend Metric"
        value={settings.totalSpendMetric}
        variant="title"
        options={[
          { value: 'cost', label: 'Cost' },
          { value: 'costPerMillion', label: 'Cost/MTok' },
          { value: 'tokens', label: 'Tokens' },
        ]}
        onChange={(value) => patch({ totalSpendMetric: value as AppSettings['totalSpendMetric'] })}
      />
      <span
        class="icon-button icon-button--plain total-card__info"
        data-tooltip={`Only includes ${providerNames.join(' and ')}.`}
        aria-label={`Only includes ${providerNames.join(' and ')}`}
        role="img"><Icon name="about" size={13} strokeWidth={1.9} /></span
      >
    </div>
    <button
      class="icon-button icon-button--plain total-card__share"
      type="button"
      aria-label={`Share ${metricTitle()} Screenshot`}
      data-tooltip="Share Screenshot"
      onclick={share}
      ><Icon name={shareCopied ? 'check' : 'share'} size={14} strokeWidth={1.8} /></button
    >
  </div>
  <div class="total-card">
    <div class="period-switcher" aria-label="Total Spend period">
      <span
        class="period-switcher__selection"
        style={`transform: translateX(${periodIndex * 100}%)`}
        aria-hidden="true"
      ></span>
      {#each [['today', 'Today'], ['yesterday', 'Yesterday'], ['last30Days', '30 Days']] as option (option[0])}
        <button
          class:active={settings.totalSpendPeriod === option[0]}
          type="button"
          onclick={() => patch({ totalSpendPeriod: option[0] as AppSettings['totalSpendPeriod'] })}
          >{option[1]}</button
        >
      {/each}
    </div>
    {#if projection.centerValue === null}
      <div class="total-card__empty">
        <span>{emptySpendMessage(settings.totalSpendMetric)}</span>
      </div>
    {:else}
      <div class="total-card__body">
        <div class="spend-total" data-tooltip={centerTooltip(projection.centerValue)}>
          <strong>{totalParts(projection.centerValue).primary}</strong><span
            >{totalParts(projection.centerValue).unit}</span
          >
        </div>
        {#if barSegments.length}
          <div class="spend-bar" role="img" aria-label="Spend share by provider">
            {#each barSegments as segment (segment.id)}
              <span
                style={`flex-grow:${segment.share};background:var(--provider-${providerFamily(segment.id)}, var(--provider))`}
                title={segmentTitle(segment.id)}
              ></span>
            {/each}
          </div>
        {/if}
        <div class="spend-legend">
          {#each projection.slices as provider (provider.id)}
            <span
              ><i
                style={`background: var(--provider-${providerFamily(provider.id)}, var(--provider))`}
              ></i>{providerDisplayName(provider.id)}</span
            ><strong>{display(provider.value, provider.period.tokens)}</strong>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</section>

<style>
  :global {
    .total-card {
      margin-bottom: 12px;
      padding: 10px 11px;
      border-radius: 12px;
      background: var(--card);
    }

    .total-card__header,
    .total-card__body,
    .trend-heading {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
    }

    .period-switcher {
      position: relative;
      display: flex;
      padding: 2px;
      border-radius: 999px;
      background: var(--meter-track);
    }

    .period-switcher button {
      position: relative;
      z-index: 1;
      padding: 3px 6px;
      border: 0;
      border-radius: 5px;
      color: var(--secondary);
      background: transparent;
      font-size: 9px;
      cursor: pointer;
    }

    .period-switcher button.active {
      color: var(--text);
      background: transparent;
      box-shadow: none;
    }

    .period-switcher__selection {
      position: absolute;
      top: 3px;
      bottom: 3px;
      left: 3px;
      width: calc((100% - 6px) / 3);
      border-radius: 999px;
      background: var(--tray);
      box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12);
      transition: transform var(--motion-spring);
    }

    .total-card__body {
      justify-content: flex-start;
      padding-top: 10px;
    }

    .spend-total {
      display: flex;
      align-items: baseline;
      gap: 4px;
    }

    .spend-total strong {
      overflow: hidden;
      font-size: var(--total-value-size);
      font-weight: 600;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .spend-total span {
      color: var(--secondary);
      font-size: var(--total-value-unit-size);
    }

    .spend-bar {
      display: flex;
      width: 100%;
      height: var(--total-bar-height);
      border-radius: 999px;
      background: var(--meter-track);
      overflow: hidden;
    }

    .spend-bar span {
      flex-basis: 0;
      min-width: 2px;
    }

    .spend-legend small {
      color: var(--secondary);
      font-size: 8px;
    }

    .spend-legend {
      display: grid;
      flex: 1;
      grid-template-columns: 1fr auto;
      gap: 2px 8px;
      font-size: 11px;
    }

    .spend-legend span i {
      display: inline-block;
      width: 7px;
      height: 7px;
      margin-right: 5px;
      border-radius: 50%;
      background: var(--provider);
    }

    .spend-legend small {
      grid-column: 1 / -1;
    }

    .icon-button {
      display: grid;
      width: 30px;
      height: 30px;
      margin-left: 0;
      padding: 0;
      border: 0;
      border-radius: 50%;
      color: var(--secondary);
      background: transparent;
      cursor: default;
      place-items: center;
    }

    .icon-button:hover,
    .icon-button:focus-visible {
      color: var(--text);
      background: var(--button-hover);
    }

    .icon-button:focus-visible {
      outline: 2px solid var(--meter-fill);
      outline-offset: 2px;
    }

    @media (prefers-reduced-motion: no-preference) {
      .icon-button {
        transition:
          color 120ms ease,
          background-color 120ms ease;
      }
    }

    .total-spend-section {
      margin-bottom: 0;
    }

    .total-card__header {
      min-height: 24px;
      margin-bottom: 4px;
      padding: 0 4px 2px;
    }

    .total-card__title {
      display: flex;
      min-width: 0;
      align-items: center;
      gap: 4px;
    }

    .select-menu--title {
      min-width: 0;
    }

    .select-menu--title .select-menu__trigger {
      min-height: 24px;
      gap: 4px;
      padding: 0;
      border: 0;
      background: transparent;
      font-size: 14px;
      font-weight: 600;
    }

    .select-menu--title .select-menu__trigger:hover,
    .select-menu--title .select-menu__trigger[aria-expanded='true'] {
      color: var(--meter-fill);
      background: transparent;
    }

    .select-menu__list--title {
      min-width: 138px;
      transform-origin: top left;
    }

    .select-menu__list--title.select-menu__list--above {
      transform-origin: bottom left;
    }

    .total-card__header .icon-button--plain {
      width: 20px;
      height: 20px;
      flex: 0 0 20px;
      color: var(--secondary);
      cursor: pointer;
    }

    .total-card__header .total-card__info {
      width: 16px;
      height: 20px;
      flex-basis: 16px;
      cursor: help;
    }

    .total-card {
      margin: 0;
      padding: var(--total-card-padding-y) var(--total-card-padding-x);
    }

    .period-switcher {
      width: 100%;
      height: var(--total-switcher-height);
      padding: 3px;
      background: var(--meter-track);
    }

    .period-switcher button {
      min-height: 0;
      flex: 1;
      padding: 0 12px;
      font-size: var(--total-period-size);
      font-weight: 500;
    }

    .period-switcher button.active {
      font-weight: 600;
    }

    .total-card__body {
      flex-direction: column;
      align-items: stretch;
      gap: var(--total-legend-gap);
      padding-top: var(--total-body-gap);
    }

    .spend-legend {
      align-content: center;
      gap: 7px 8px;
      font-size: var(--total-legend-size);
    }

    .spend-legend span i {
      width: 8px;
      height: 8px;
      margin-right: 7px;
    }

    .spend-legend strong {
      color: var(--secondary);
      font-weight: 500;
      text-align: right;
    }

    .total-card__empty {
      display: flex;
      min-height: 76px;
      align-items: center;
      justify-content: center;
      flex-direction: column;
      gap: 8px;
      color: var(--secondary);
      font-size: 11px;
      text-align: center;
    }

    :root[data-density='compact'] .total-card {
      padding: 10px 12px;
    }

    :root[data-density='compact'] .period-switcher button {
      min-height: 21px;
      padding: 3px 10px;
    }

    :root[data-density='compact'] .total-card__body {
      gap: 14px;
      padding-top: 8px;
    }

    :root[data-density='compact'] .total-card__empty {
      min-height: 64px;
    }
  }
</style>
