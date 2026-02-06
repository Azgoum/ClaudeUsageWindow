"""
Claude Token Monitor - Always-on-top widget with WhatsApp notification
"""
import tkinter as tk
from tkinter import ttk, messagebox
import json
import os
import subprocess
import threading
import re
from datetime import datetime, timedelta
from pathlib import Path
import time

class TokenMonitor:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("Claude Tokens")
        self.root.attributes('-topmost', True)
        self.root.overrideredirect(False)

        # Window settings
        self.root.geometry("300x180+50+50")
        self.root.resizable(True, True)
        self.root.configure(bg='#1a1a2e')

        # State file for persistence
        self.state_file = Path(__file__).parent / "state.json"
        self.load_state()

        # Minimize state
        self.is_minimized = False
        self.normal_geometry = None

        # Timer thread
        self.timer_thread = None
        self.timer_running = False

        self.setup_ui()
        self.setup_drag()
        self.update_display()

        # Check if notification is pending
        self.check_pending_notification()

    def load_state(self):
        """Load saved state"""
        try:
            if self.state_file.exists():
                with open(self.state_file, 'r') as f:
                    data = json.load(f)
                    self.limit_hit_time = data.get('limit_hit_time')
                    self.reset_time = data.get('reset_time')
                    self.whatsapp_number = data.get('whatsapp_number', '+33XXXXXXXXX')
                    self.notification_sent = data.get('notification_sent', False)
            else:
                self.limit_hit_time = None
                self.reset_time = None
                self.whatsapp_number = '+33XXXXXXXXX'
                self.notification_sent = False
        except Exception:
            self.limit_hit_time = None
            self.reset_time = None
            self.whatsapp_number = '+33XXXXXXXXX'
            self.notification_sent = False

    def save_state(self):
        """Save state to file"""
        try:
            with open(self.state_file, 'w') as f:
                json.dump({
                    'limit_hit_time': self.limit_hit_time,
                    'reset_time': self.reset_time,
                    'whatsapp_number': self.whatsapp_number,
                    'notification_sent': self.notification_sent
                }, f)
        except Exception as e:
            print(f"Error saving state: {e}")

    def setup_ui(self):
        # Main frame
        self.main_frame = tk.Frame(self.root, bg='#1a1a2e')
        self.main_frame.pack(fill='both', expand=True, padx=5, pady=5)

        # Title bar with controls
        title_frame = tk.Frame(self.main_frame, bg='#16213e')
        title_frame.pack(fill='x', pady=(0, 5))

        title_label = tk.Label(
            title_frame,
            text="🦀 Claude Tokens",
            bg='#16213e',
            fg='#e94560',
            font=('Segoe UI', 10, 'bold')
        )
        title_label.pack(side='left', padx=5, pady=2)

        # Minimize button
        self.min_btn = tk.Button(
            title_frame,
            text="─",
            command=self.toggle_minimize,
            bg='#16213e',
            fg='#ffffff',
            font=('Segoe UI', 8),
            relief='flat',
            width=2,
            cursor='hand2'
        )
        self.min_btn.pack(side='right', padx=2, pady=2)

        # Content frame (collapsible)
        self.content_frame = tk.Frame(self.main_frame, bg='#1a1a2e')
        self.content_frame.pack(fill='both', expand=True)

        # Status display
        self.status_label = tk.Label(
            self.content_frame,
            text="Status: Active",
            bg='#1a1a2e',
            fg='#4ecca3',
            font=('Segoe UI', 11, 'bold')
        )
        self.status_label.pack(pady=5)

        # Timer display
        self.timer_label = tk.Label(
            self.content_frame,
            text="",
            bg='#1a1a2e',
            fg='#f4a261',
            font=('Segoe UI', 14, 'bold')
        )
        self.timer_label.pack(pady=5)

        # Reset time display
        self.reset_label = tk.Label(
            self.content_frame,
            text="",
            bg='#1a1a2e',
            fg='#a0a0a0',
            font=('Segoe UI', 9)
        )
        self.reset_label.pack(pady=2)

        # Buttons frame
        btn_frame = tk.Frame(self.content_frame, bg='#1a1a2e')
        btn_frame.pack(fill='x', pady=10)

        # "Limit reached" button
        self.limit_btn = tk.Button(
            btn_frame,
            text="⚠️ Limite atteinte",
            command=self.on_limit_reached,
            bg='#e94560',
            fg='#ffffff',
            font=('Segoe UI', 9, 'bold'),
            relief='flat',
            cursor='hand2',
            padx=10
        )
        self.limit_btn.pack(side='left', padx=5)

        # "Clear" button
        self.clear_btn = tk.Button(
            btn_frame,
            text="✓ Reset",
            command=self.on_clear,
            bg='#4ecca3',
            fg='#1a1a2e',
            font=('Segoe UI', 9, 'bold'),
            relief='flat',
            cursor='hand2',
            padx=10
        )
        self.clear_btn.pack(side='right', padx=5)

    def setup_drag(self):
        """Enable window dragging"""
        self.root.bind('<Button-1>', self.start_drag)
        self.root.bind('<B1-Motion>', self.do_drag)

    def start_drag(self, event):
        self.drag_x = event.x
        self.drag_y = event.y

    def do_drag(self, event):
        x = self.root.winfo_x() + event.x - self.drag_x
        y = self.root.winfo_y() + event.y - self.drag_y
        self.root.geometry(f"+{x}+{y}")

    def toggle_minimize(self):
        if self.is_minimized:
            # Restore
            self.content_frame.pack(fill='both', expand=True)
            self.root.geometry(self.normal_geometry)
            self.min_btn.config(text="─")
            self.is_minimized = False
        else:
            # Minimize
            self.normal_geometry = self.root.geometry()
            self.content_frame.pack_forget()
            geo = self.root.geometry()
            match = re.match(r'(\d+)x(\d+)\+(-?\d+)\+(-?\d+)', geo)
            if match:
                x, y = match.group(3), match.group(4)
                self.root.geometry(f"160x30+{x}+{y}")
            self.min_btn.config(text="□")
            self.is_minimized = True

    def on_limit_reached(self):
        """User clicked 'Limit reached' button"""
        now = datetime.now()
        self.limit_hit_time = now.isoformat()
        reset = now + timedelta(hours=5)
        self.reset_time = reset.isoformat()
        self.notification_sent = False
        self.save_state()

        self.status_label.config(text="⏳ En attente...", fg='#e94560')
        self.update_display()
        self.start_timer()

    def on_clear(self):
        """Clear the limit state"""
        self.limit_hit_time = None
        self.reset_time = None
        self.notification_sent = False
        self.timer_running = False
        self.save_state()

        self.status_label.config(text="Status: Active", fg='#4ecca3')
        self.timer_label.config(text="")
        self.reset_label.config(text="")

    def update_display(self):
        """Update the display based on current state"""
        if self.reset_time:
            reset = datetime.fromisoformat(self.reset_time)
            self.reset_label.config(text=f"Reset prévu: {reset.strftime('%H:%M')}")

    def start_timer(self):
        """Start the countdown timer"""
        if self.timer_running:
            return

        self.timer_running = True

        def timer_loop():
            while self.timer_running and self.reset_time:
                try:
                    reset = datetime.fromisoformat(self.reset_time)
                    now = datetime.now()
                    remaining = reset - now

                    if remaining.total_seconds() <= 0:
                        # Time's up!
                        self.root.after(0, self.on_reset_reached)
                        break

                    hours, remainder = divmod(int(remaining.total_seconds()), 3600)
                    minutes, seconds = divmod(remainder, 60)

                    def update_timer():
                        self.timer_label.config(text=f"{hours:02d}:{minutes:02d}:{seconds:02d}")

                    self.root.after(0, update_timer)
                    time.sleep(1)

                except Exception as e:
                    print(f"Timer error: {e}")
                    break

        self.timer_thread = threading.Thread(target=timer_loop, daemon=True)
        self.timer_thread.start()

    def on_reset_reached(self):
        """Called when the reset time is reached"""
        self.timer_running = False
        self.status_label.config(text="✅ Tokens disponibles!", fg='#4ecca3')
        self.timer_label.config(text="00:00:00")

        # Send WhatsApp notification if not already sent
        if not self.notification_sent:
            self.send_whatsapp_notification()
            self.notification_sent = True
            self.save_state()

    def send_whatsapp_notification(self):
        """Send WhatsApp notification via OpenClaw"""
        try:
            message = "🦀 Tes tokens Claude sont de nouveau disponibles! Tu peux reprendre ta session."

            cmd = [
                'openclaw', 'message', 'send',
                '--channel', 'whatsapp',
                '--target', self.whatsapp_number,
                '--message', message
            ]

            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                timeout=30,
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            )

            if result.returncode == 0:
                print("WhatsApp notification sent successfully")
            else:
                print(f"WhatsApp notification failed: {result.stderr}")
                # Show error to user
                messagebox.showwarning(
                    "Notification",
                    f"Tokens disponibles!\n\nErreur WhatsApp: {result.stderr[:100]}"
                )

        except FileNotFoundError:
            messagebox.showwarning(
                "Notification",
                "Tokens disponibles!\n\nOpenClaw non trouvé - notification WhatsApp impossible."
            )
        except Exception as e:
            print(f"Error sending notification: {e}")
            messagebox.showwarning(
                "Notification",
                f"Tokens disponibles!\n\nErreur: {str(e)[:100]}"
            )

    def check_pending_notification(self):
        """Check if we need to resume a timer or send notification"""
        if self.reset_time and not self.notification_sent:
            reset = datetime.fromisoformat(self.reset_time)
            now = datetime.now()

            if now >= reset:
                # Reset time passed while app was closed
                self.on_reset_reached()
            else:
                # Resume timer
                self.status_label.config(text="⏳ En attente...", fg='#e94560')
                self.update_display()
                self.start_timer()

    def run(self):
        self.root.mainloop()


if __name__ == "__main__":
    app = TokenMonitor()
    app.run()
