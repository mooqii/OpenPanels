# NumberInput

A composeable number input component with drag-to-adjust functionality for the canvas toolbar.

## Installation

```tsx
import { NumberInput } from '@/canvas/components/number-input'
```

## Basic Usage

```tsx
<NumberInput value={width} onChange={setWidth}>
  <NumberInput.Input />
  <NumberInput.Unit>px</NumberInput.Unit>
  <NumberInput.DragHandle />
</NumberInput>
```

## API

### NumberInput

Root component that provides context to sub-components.

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `value` | `number` | - | Current numeric value (required) |
| `onChange` | `(value: number) => void` | - | Callback when value changes (required) |
| `min` | `number` | - | Minimum allowed value |
| `max` | `number` | - | Maximum allowed value |
| `step` | `number` | `1` | Increment step for keyboard arrows |
| `precision` | `number` | `auto` | Decimal places for display |
| `unit` | `string` | - | Fallback unit if Unit component not used |
| `dragVelocity` | `number` | `0.1` | Drag sensitivity multiplier |
| `disabled` | `boolean` | `false` | Disabled state |
| `className` | `string` | - | Container className |
| `children` | `ReactNode` | - | Sub-components for composition |

### NumberInput.Input

The number input element.

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `className` | `string` | - | Input element className |

### NumberInput.Unit

Unit suffix display.

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `children` | `ReactNode` | - | Unit display content |
| `className` | `string` | - | Unit element className |

### NumberInput.DragHandle

Drag handle for value adjustment.

| Prop | Type | Default | Description |
|------|------|---------|-------------|
| `icon` | `ReactNode` | `⠿` | Custom drag icon |
| `className` | `string` | - | Drag handle className |

## Examples

### Width/Height Input

```tsx
<NumberInput value={width} onChange={setWidth} min={0} max={1920}>
  <NumberInput.Input />
  <NumberInput.Unit>px</NumberInput.Unit>
  <NumberInput.DragHandle />
</NumberInput>
```

### Rotation with Decimals

```tsx
<NumberInput value={rotation} onChange={setRotation} min={0} max={360} step={0.1} precision={1}>
  <NumberInput.Input />
  <NumberInput.Unit>°</NumberInput.Unit>
  <NumberInput.DragHandle />
</NumberInput>
```

### Opacity (0-1 Range)

```tsx
<NumberInput value={opacity} onChange={setOpacity} min={0} max={1} step={0.01} precision={2}>
  <NumberInput.Input />
  <NumberInput.Unit />
  <NumberInput.DragHandle />
</NumberInput>
```

### Compact Mode

```tsx
<NumberInput value={value} onChange={setValue}>
  <NumberInput.Input className="w-12 bg-transparent" />
  <NumberInput.Unit className="text-[10px]">px</NumberInput.Unit>
  <NumberInput.DragHandle className="w-3 h-3" />
</NumberInput>
```

## Keyboard Shortcuts

| Key | Action | Shift |
|-----|--------|-------|
| `↑` / `ArrowUp` | `value + step` | `value + step * 10` |
| `↓` / `ArrowDown` | `value - step` | `value - step * 10` |
| `Home` | Set to `min` value | - |
| `End` | Set to `max` value | - |
