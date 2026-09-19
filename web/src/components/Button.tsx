import type { ButtonHTMLAttributes } from 'react'

type Variant = 'default' | 'primary' | 'ghost'

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant
}

export function Button({ variant = 'default', className = '', type = 'button', ...rest }: Props) {
  return <button type={type} className={`btn btn--${variant} ${className}`.trim()} {...rest} />
}
