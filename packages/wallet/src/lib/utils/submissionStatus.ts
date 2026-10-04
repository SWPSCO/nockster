export function submissionStatusLabel(status?: string): string {
  switch (status) {
    case 'submitting':
      return 'Submitting';
    case 'acknowledged':
      return 'Submitted';
    case 'accepted':
      return 'Accepted';
    case 'confirmed':
      return 'Confirmed';
    case 'rejected':
      return 'Not accepted';
    default:
      return 'Pending';
  }
}
