use colored::Colorize;
use tokio::sync::mpsc;

use crate::diff_finder::Difference;

pub struct DifferencesPrinter {
    receiver: mpsc::Receiver<DifferencesPrinterMessage>,
    done_signal: tokio::sync::oneshot::Sender<()>,
}
pub enum DifferencesPrinterMessage {
    PrintDifferences {
        differences: Vec<Difference>,
        request_id: String,
        url: String,
    },
}

impl DifferencesPrinter {
    pub fn new(
        receiver: mpsc::Receiver<DifferencesPrinterMessage>,
        done_signal: tokio::sync::oneshot::Sender<()>,
    ) -> Self {
        DifferencesPrinter {
            receiver,
            done_signal,
        }
    }
    fn handle_message(&mut self, msg: DifferencesPrinterMessage) {
        match msg {
            DifferencesPrinterMessage::PrintDifferences {
                differences,
                request_id,
                url,
            } => {
                assert!(!differences.is_empty());

                println!(
                    "\n❌-----------------------------------------------------------------------------------------❌"
                );
                println!(
                    "{}",
                    format!(
                        "Differences detected for request '{}' of URL '{}'",
                        request_id, url
                    )
                    .yellow()
                );

                for diff in &differences {
                    diff.print();
                }

                println!(
                    "❌-----------------------------------------------------------------------------------------❌"
                );
            }
        }
    }
}

pub async fn run_differences_printer(mut actor: DifferencesPrinter) {
    while let Some(msg) = actor.receiver.recv().await {
        actor.handle_message(msg);
    }

    // Signal we're done
    let _ = actor.done_signal.send(());
}
