from PyQt6 import QtWidgets
from PyQt6.QtCore import pyqtSlot


class InfoBox(QtWidgets.QMessageBox):
    def __init__(self):
        super().__init__()
        self.setIcon(QtWidgets.QMessageBox.Icon.Information)

    @pyqtSlot(str, str)
    def display_info_box(self, title, text):
        self.setWindowTitle(title)
        self.setText(text)
        self.show()
