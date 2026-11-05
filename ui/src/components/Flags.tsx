import type { Lang } from '@/lib/types'

interface Props {
  lang: Lang
  onLang: (l: Lang) => void
}

/** FR / EN-GB flags as SVG (flag emojis don't render on Windows), copied from the prototype. */
export function Flags({ lang, onLang }: Props) {
  const pick = (l: Lang) => () => {
    if (l !== lang) onLang(l)
  }
  return (
    <div className="flags" data-nodrag>
      <svg data-lang="fr" viewBox="0 0 3 2" className={lang === 'fr' ? 'on' : undefined} onClick={pick('fr')}>
        <title>Français</title>
        <rect width="1" height="2" fill="#0055A4" />
        <rect x="1" width="1" height="2" fill="#fff" />
        <rect x="2" width="1" height="2" fill="#EF4135" />
      </svg>
      <svg data-lang="en" viewBox="0 0 60 30" className={lang === 'en' ? 'on' : undefined} onClick={pick('en')}>
        <title>English (UK)</title>
        <clipPath id="uk-t">
          <path d="M30,15 h30 v15 z v15 h-30 z h-30 v-15 z v-15 h30 z" />
        </clipPath>
        <path d="M0,0 v30 h60 v-30 z" fill="#012169" />
        <path d="M0,0 L60,30 M60,0 L0,30" stroke="#fff" strokeWidth="6" />
        <path d="M0,0 L60,30 M60,0 L0,30" clipPath="url(#uk-t)" stroke="#C8102E" strokeWidth="4" />
        <path d="M30,0 v30 M0,15 h60" stroke="#fff" strokeWidth="10" />
        <path d="M30,0 v30 M0,15 h60" stroke="#C8102E" strokeWidth="6" />
      </svg>
    </div>
  )
}
