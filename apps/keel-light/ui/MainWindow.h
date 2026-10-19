#pragma once

#include <QMainWindow>
#include <QTimer>

class RpcClient;
class QLabel;
class QListWidget;
class QSlider;
class QCheckBox;
class QPushButton;

class MainWindow : public QMainWindow {
  Q_OBJECT
public:
  explicit MainWindow(quint16 port, QWidget *parent = nullptr);

private slots:
  void refreshLights();
  void onConnected(bool on);
  void onSelectionChanged();
  void onPowerToggled(bool on);
  void onBrightnessChanged(int value);
  void onTemperatureChanged(int mired);
  void applyControls();

private:
  void buildUi();
  QString selectedId() const;

  quint16 port_;
  RpcClient *rpc_;
  QLabel *status_;
  QListWidget *list_;
  QCheckBox *power_;
  QSlider *brightness_;
  QSlider *temperature_;
  QLabel *brightnessLabel_;
  QLabel *temperatureLabel_;
  QPushButton *refreshBtn_;
  QTimer refreshTimer_;
  bool applying_ = false;
};
