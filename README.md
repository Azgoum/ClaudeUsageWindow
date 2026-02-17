# ClaudeUsageWindow

Widget Windows always-on-top affichant ta consommation Claude.ai en temps réel, exactement comme sur https://claude.ai/settings/usage

## Fonctionnalités

- **Affichage temps réel** : Session (5h) et usage hebdomadaire
- **Barres de progression** colorées selon le niveau (vert → orange → rouge)
- **Indicateur de rythme** : "En avance" (rouge), "On track" (vert), "En retard" (orange) par rapport au rythme idéal de consommation
- **Always-on-top** : toujours visible sur ton bureau
- **Notification WhatsApp** : reçois un message quand tes tokens sont dispo (via OpenClaw)
- **Auto-refresh** : mise à jour toutes les 2 minutes
- **Icône Claude** : icône PNG personnalisée pour la fenêtre et la barre des tâches

## Prérequis

- Python 3.8+
- Firefox avec une session active sur claude.ai
- [OpenClaw](https://openclaw.ai) configuré avec WhatsApp (optionnel, pour les notifications)

## Installation

```bash
git clone https://github.com/Azgoum/ClaudeUsageWindow.git
cd ClaudeUsageWindow
pip install -r requirements.txt
```

## Utilisation

### Lancement

Double-cliquer sur `start.bat` ou :

```bash
python token_monitor.py
```

### Important

**Tu dois être connecté sur claude.ai dans Firefox** pour que le widget puisse récupérer les données. Le widget utilise les cookies de ta session Firefox.

### Fonctionnement

1. Le widget récupère les données depuis l'API claude.ai
2. Affiche le % d'utilisation session et hebdomadaire
3. Affiche l'heure du prochain reset
4. Quand l'usage atteint 95%, une notification WhatsApp est programmée pour le reset

## Configuration

Créez un fichier `config.json` à la racine du projet (voir `config.example.json`) :

```json
{
  "whatsapp_number": "+33XXXXXXXXX"
}
```

Le numéro peut aussi être défini via la variable d'environnement `WHATSAPP_NUMBER`.

## Fichiers

```
ClaudeUsageWindow/
├── token_monitor.py       # Application principale
├── start.bat              # Lanceur Windows (sans fenêtre CMD)
├── icons8-claude-48.png   # Icône Claude
├── requirements.txt       # Dépendances Python
├── state.json             # État persistant (auto-généré)
├── .gitignore
└── README.md
```

## API Claude.ai

Le widget utilise l'endpoint non-documenté :
```
GET https://claude.ai/api/organizations/{uuid}/usage
```

## Dépendances

- **browser-cookie3** : pour lire les cookies Firefox
- **OpenClaw** (optionnel) : pour les notifications WhatsApp

## Licence

MIT
